use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use reqwest::{Client, Method, Response, header::HeaderMap, redirect::Policy};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use tokio::sync::RwLock;
use url::Url;

use crate::config::Config;

#[derive(Clone)]
pub struct JenkinsClient {
    config: Arc<Config>,
    crumb_cache: Arc<RwLock<HashMap<String, CachedCrumb>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JenkinsItem {
    pub name: String,
    #[serde(rename = "fullName", default)]
    pub full_name: String,
    pub url: String,
    #[serde(rename = "_class", default)]
    pub class_name: String,
    #[serde(default)]
    pub jobs: Vec<JenkinsItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JenkinsRoot {
    #[serde(default)]
    pub jobs: Vec<JenkinsItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Crumb {
    #[serde(rename = "crumbRequestField")]
    pub field: String,
    pub crumb: String,
}

#[derive(Clone)]
struct CrumbContext {
    value: Crumb,
    cookie: Option<String>,
}

struct CachedCrumb {
    base: String,
    context: CrumbContext,
    expires_at: Instant,
}

const CRUMB_CACHE_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: Option<i64>,
    pub cancelled: Option<bool>,
    pub executable: Option<Executable>,
    pub why: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Executable {
    pub number: i64,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildInfo {
    pub building: bool,
    pub result: Option<String>,
    pub number: i64,
    pub url: String,
}

impl JenkinsClient {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            crumb_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn validate_url(&self, raw: &str) -> Result<Url> {
        let url = Url::parse(raw).context("Jenkins 地址格式错误")?;
        if !matches!(url.scheme(), "http" | "https") {
            bail!("Jenkins 地址仅支持 http 或 https");
        }
        if !url.username().is_empty() || url.password().is_some() {
            bail!("Jenkins 地址不能包含凭据");
        }
        url.host_str().context("Jenkins 地址缺少主机")?;
        Ok(url)
    }

    fn client(&self, timeout: u64, invalid_certs: bool) -> Result<Client> {
        Ok(Client::builder()
            .redirect(Policy::none())
            .timeout(Duration::from_secs(timeout))
            .danger_accept_invalid_certs(invalid_certs)
            .user_agent("goodnight/0.1")
            .build()?)
    }

    async fn request(
        &self,
        method: Method,
        base: &str,
        path: &str,
        timeout: u64,
        invalid_certs: bool,
    ) -> Result<reqwest::RequestBuilder> {
        let mut base = self.validate_url(base).await?;
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        let target = base
            .join(path.trim_start_matches('/'))
            .context("Jenkins API 路径无效")?;
        Ok(self
            .client(timeout, invalid_certs)?
            .request(method, target)
            .basic_auth(
                &self.config.jenkins_username,
                Some(&self.config.jenkins_password),
            ))
    }

    async fn json<T: DeserializeOwned>(
        &self,
        base: &str,
        path: &str,
        timeout: u64,
        invalid_certs: bool,
    ) -> Result<T> {
        let request = self
            .request(Method::GET, base, path, timeout, invalid_certs)
            .await?
            .build()?;
        log_request(&request);
        let response = self
            .client(timeout, invalid_certs)?
            .execute(request)
            .await?;
        let status = response.status();
        let url = response.url().to_string();
        let headers = response.headers().clone();
        let body = response.bytes().await?;
        log_response(status.as_u16(), &url, &headers, &body);
        if !status.is_success() {
            bail!(
                "Jenkins 请求失败：HTTP {}，响应：{}",
                status,
                String::from_utf8_lossy(&body)
            );
        }
        serde_json::from_slice(&body).context("Jenkins 返回了无效 JSON")
    }

    pub async fn test(&self, base: &str, timeout: u64, invalid_certs: bool) -> Result<()> {
        let _: Value = self.json(base, "api/json", timeout, invalid_certs).await?;
        Ok(())
    }

    pub async fn folders(
        &self,
        base: &str,
        timeout: u64,
        invalid_certs: bool,
    ) -> Result<Vec<JenkinsItem>> {
        let root: JenkinsRoot = self.json(base, "api/json?tree=jobs[name,fullName,url,_class,jobs[name,fullName,url,_class,jobs[name,fullName,url,_class,jobs[name,fullName,url,_class]]]]", timeout, invalid_certs).await?;
        Ok(folder_items(root.jobs))
    }

    pub async fn workflow_jobs(
        &self,
        folder_url: &str,
        timeout: u64,
        invalid_certs: bool,
    ) -> Result<Vec<JenkinsItem>> {
        let root: JenkinsRoot = self
            .json(
                folder_url,
                "api/json?tree=jobs[name,fullName,url,_class]",
                timeout,
                invalid_certs,
            )
            .await?;
        Ok(root
            .jobs
            .into_iter()
            .filter(|item| item.class_name.to_ascii_lowercase().contains("workflowjob"))
            .collect())
    }

    pub async fn job_definition_at(
        &self,
        job_url: &str,
        timeout: u64,
        invalid_certs: bool,
    ) -> Result<Value> {
        self.json(job_url, "api/json", timeout, invalid_certs).await
    }

    pub async fn job_definition(
        &self,
        base: &str,
        full_name: &str,
        timeout: u64,
        invalid_certs: bool,
    ) -> Result<Value> {
        self.json(
            base,
            &format!("{}/api/json", job_path(full_name)),
            timeout,
            invalid_certs,
        )
        .await
    }

    async fn crumb(&self, base: &str, timeout: u64, invalid_certs: bool) -> Result<CrumbContext> {
        let username = self.config.jenkins_username.clone();
        let normalized_base = base.trim_end_matches('/').to_owned();
        if let Some(context) = self
            .crumb_cache
            .read()
            .await
            .get(&username)
            .filter(|cached| cached.base == normalized_base && cached.expires_at > Instant::now())
            .map(|cached| cached.context.clone())
        {
            return Ok(context);
        }
        let request = self
            .request(
                Method::GET,
                base,
                "crumbIssuer/api/json",
                timeout,
                invalid_certs,
            )
            .await?
            .build()?;
        log_request(&request);
        let response = self
            .client(timeout, invalid_certs)?
            .execute(request)
            .await?;
        let status = response.status();
        let url = response.url().to_string();
        let headers = response.headers().clone();
        let cookie = response
            .headers()
            .get_all(reqwest::header::SET_COOKIE)
            .iter()
            .filter_map(|value| value.to_str().ok())
            .filter_map(|value| value.split(';').next())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join("; ");
        let body = response.bytes().await?;
        log_response(status.as_u16(), &url, &headers, &body);
        if !status.is_success() {
            bail!(
                "Jenkins crumb 获取失败：HTTP {}，响应：{}",
                status,
                String::from_utf8_lossy(&body)
            );
        }
        let context = CrumbContext {
            value: serde_json::from_slice(&body).context("Jenkins crumb 响应格式无效")?,
            cookie: (!cookie.is_empty()).then_some(cookie),
        };
        self.crumb_cache.write().await.insert(
            username,
            CachedCrumb {
                base: normalized_base,
                context: context.clone(),
                expires_at: Instant::now() + CRUMB_CACHE_TTL,
            },
        );
        Ok(context)
    }

    pub async fn trigger(
        &self,
        base: &str,
        full_name: &str,
        parameters: &Value,
        timeout: u64,
        invalid_certs: bool,
    ) -> Result<String> {
        let mut request = self
            .request(
                Method::POST,
                base,
                &format!("{}/buildWithParameters", job_path(full_name)),
                timeout,
                invalid_certs,
            )
            .await?;
        let crumb = self
            .crumb(base, timeout, invalid_certs)
            .await
            .context("调用 Jenkins 前获取 crumb 失败")?;
        request = apply_crumb(request, crumb);
        if let Some(values) = parameters.as_object() {
            request = request.form(
                &values
                    .iter()
                    .map(|(k, v)| (k, scalar(v)))
                    .collect::<Vec<_>>(),
            );
        }
        let request = request.build()?;
        log_request(&request);
        let response = checked(
            self.client(timeout, invalid_certs)?
                .execute(request)
                .await?,
        )
        .await?;
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .context("Jenkins 未返回 queue Location")?;
        Ok(response.url().join(location)?.to_string())
    }

    pub async fn trigger_at(
        &self,
        job_url: &str,
        jenkins_url: &str,
        parameters: &Value,
        timeout: u64,
        invalid_certs: bool,
    ) -> Result<String> {
        let mut request = self
            .request(
                Method::POST,
                job_url,
                "buildWithParameters",
                timeout,
                invalid_certs,
            )
            .await?;
        let crumb = self
            .crumb(jenkins_url, timeout, invalid_certs)
            .await
            .context("调用 Jenkins 前获取 crumb 失败")?;
        request = apply_crumb(request, crumb);
        if let Some(values) = parameters.as_object() {
            request = request.form(
                &values
                    .iter()
                    .map(|(key, value)| (key, scalar(value)))
                    .collect::<Vec<_>>(),
            );
        }
        let request = request.build()?;
        log_request(&request);
        let response = checked(
            self.client(timeout, invalid_certs)?
                .execute(request)
                .await?,
        )
        .await?;
        let location = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .context("Jenkins 未返回 queue Location")?;
        Ok(response.url().join(location)?.to_string())
    }

    pub async fn queue(
        &self,
        base: &str,
        queue_id: i64,
        timeout: u64,
        invalid_certs: bool,
    ) -> Result<QueueItem> {
        self.json(
            base,
            &format!("queue/item/{queue_id}/api/json"),
            timeout,
            invalid_certs,
        )
        .await
    }

    pub async fn build(
        &self,
        base: &str,
        full_name: &str,
        number: i64,
        timeout: u64,
        invalid_certs: bool,
    ) -> Result<BuildInfo> {
        self.json(
            base,
            &format!("{}/{number}/api/json", job_path(full_name)),
            timeout,
            invalid_certs,
        )
        .await
    }

    pub async fn progressive_log(
        &self,
        base: &str,
        full_name: &str,
        number: i64,
        offset: i64,
        timeout: u64,
        invalid_certs: bool,
    ) -> Result<(String, i64, bool)> {
        let response = self
            .request(
                Method::GET,
                base,
                &format!(
                    "{}/{number}/logText/progressiveText?start={offset}",
                    job_path(full_name)
                ),
                timeout,
                invalid_certs,
            )
            .await?
            .send()
            .await?;
        let response = checked(response).await?;
        let next = response
            .headers()
            .get("X-Text-Size")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse().ok())
            .unwrap_or(offset);
        let more = response
            .headers()
            .get("X-More-Data")
            .and_then(|v| v.to_str().ok())
            == Some("true");
        Ok((response.text().await?, next, more))
    }

    pub async fn cancel_queue(
        &self,
        base: &str,
        queue_id: i64,
        timeout: u64,
        invalid_certs: bool,
    ) -> Result<()> {
        let mut request = self
            .request(
                Method::POST,
                base,
                &format!("queue/cancelItem?id={queue_id}"),
                timeout,
                invalid_certs,
            )
            .await?;
        if let Ok(crumb) = self.crumb(base, timeout, invalid_certs).await {
            request = apply_crumb(request, crumb);
        }
        checked(request.send().await?).await?;
        Ok(())
    }

    pub async fn stop_build(
        &self,
        base: &str,
        full_name: &str,
        number: i64,
        timeout: u64,
        invalid_certs: bool,
    ) -> Result<()> {
        let mut request = self
            .request(
                Method::POST,
                base,
                &format!("{}/{number}/stop", job_path(full_name)),
                timeout,
                invalid_certs,
            )
            .await?;
        if let Ok(crumb) = self.crumb(base, timeout, invalid_certs).await {
            request = apply_crumb(request, crumb);
        }
        checked(request.send().await?).await?;
        Ok(())
    }
}

async fn checked(response: Response) -> Result<Response> {
    if response.status().is_success() {
        tracing::info!(
            target: "goodnight::jenkins::wire",
            status = response.status().as_u16(),
            url = %response.url(),
            headers = %headers_for_log(response.headers()),
            "Jenkins response"
        );
        Ok(response)
    } else {
        let status = response.status();
        let url = response.url().to_string();
        let headers = response.headers().clone();
        let body = response.bytes().await?;
        log_response(status.as_u16(), &url, &headers, &body);
        bail!(
            "Jenkins 请求失败：HTTP {}，响应：{}",
            status,
            String::from_utf8_lossy(&body)
        )
    }
}

fn log_request(request: &reqwest::Request) {
    let body = request
        .body()
        .and_then(|body| body.as_bytes())
        .map(String::from_utf8_lossy)
        .unwrap_or_default();
    tracing::info!(
        target: "goodnight::jenkins::wire",
        method = %request.method(),
        url = %request.url(),
        headers = %headers_for_log(request.headers()),
        body = %body,
        "Jenkins request"
    );
}

fn log_response(status: u16, url: &str, headers: &HeaderMap, body: &[u8]) {
    tracing::info!(
        target: "goodnight::jenkins::wire",
        status,
        url,
        headers = %headers_for_log(headers),
        body = %String::from_utf8_lossy(body),
        "Jenkins response"
    );
}

fn headers_for_log(headers: &HeaderMap) -> String {
    let values = headers
        .iter()
        .map(|(name, value)| {
            let sensitive = matches!(
                name.as_str(),
                "authorization" | "cookie" | "set-cookie" | "x-jenkins-crumb"
            );
            (
                name.as_str().to_owned(),
                Value::String(if sensitive {
                    "***".to_owned()
                } else {
                    value.to_str().unwrap_or("<binary>").to_owned()
                }),
            )
        })
        .collect::<serde_json::Map<_, _>>();
    Value::Object(values).to_string()
}

fn job_path(full_name: &str) -> String {
    full_name
        .split('/')
        .map(|segment| {
            format!(
                "job/{}",
                percent_encoding::utf8_percent_encode(segment, percent_encoding::NON_ALPHANUMERIC)
            )
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn scalar(value: &Value) -> String {
    match value {
        Value::String(v) => v.clone(),
        Value::Bool(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        _ => value.to_string(),
    }
}

fn apply_crumb(
    mut request: reqwest::RequestBuilder,
    crumb: CrumbContext,
) -> reqwest::RequestBuilder {
    request = request.header(crumb.value.field, crumb.value.crumb);
    if let Some(cookie) = crumb.cookie {
        request = request.header(reqwest::header::COOKIE, cookie);
    }
    request
}

fn folder_items(items: Vec<JenkinsItem>) -> Vec<JenkinsItem> {
    items
        .into_iter()
        .filter_map(|mut item| {
            item.jobs = folder_items(item.jobs);
            item.class_name
                .to_ascii_lowercase()
                .contains("folder")
                .then_some(item)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Crumb, CrumbContext, JenkinsItem, apply_crumb, folder_items, job_path};
    #[test]
    fn job_names_are_encoded_by_segment() {
        assert_eq!(job_path("A folder/deploy"), "job/A%20folder/job/deploy");
    }

    #[test]
    fn folder_discovery_excludes_jobs() {
        let items = vec![
            JenkinsItem {
                name: "folder".into(),
                full_name: "folder".into(),
                url: "https://jenkins/job/folder/".into(),
                class_name: "com.cloudbees.hudson.plugins.folder.Folder".into(),
                jobs: vec![],
                error: None,
            },
            JenkinsItem {
                name: "pipeline".into(),
                full_name: "pipeline".into(),
                url: "https://jenkins/job/pipeline/".into(),
                class_name: "org.jenkinsci.plugins.workflow.job.WorkflowJob".into(),
                jobs: vec![],
                error: None,
            },
        ];
        assert_eq!(folder_items(items).len(), 1);
    }

    #[test]
    fn crumb_header_and_session_cookie_are_forwarded() {
        let request = apply_crumb(
            reqwest::Client::new().post("https://jenkins.example/job/demo/buildWithParameters"),
            CrumbContext {
                value: Crumb {
                    field: "Jenkins-Crumb".into(),
                    crumb: "crumb-value".into(),
                },
                cookie: Some("JSESSIONID=session-value".into()),
            },
        )
        .build()
        .unwrap();
        assert_eq!(request.headers()["Jenkins-Crumb"], "crumb-value");
        assert_eq!(
            request.headers()[reqwest::header::COOKIE],
            "JSESSIONID=session-value"
        );
    }
}

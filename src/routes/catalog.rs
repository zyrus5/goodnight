use axum::{
    Json,
    extract::{Path, Query, State},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    app::AppState,
    audit,
    auth::{CurrentUser, component_permission},
    error::{ApiError, ApiResult},
    models::{
        ComponentView, CustomerView, EnvironmentQuery, EnvironmentView, InstanceQuery,
        InstanceView, JobConfigView, Page, PageQuery,
    },
};

const COMPONENT_SELECT: &str = "SELECT c.id,c.code,c.name,c.is_public,c.is_active,c.version,COALESCE(string_agg(DISTINCT u.display_name,', ') FILTER(WHERE m.role='OWNER'),'') owner_names,COALESCE(string_agg(DISTINCT u.display_name,', ') FILTER(WHERE m.role='DEVELOPER'),'') developer_names,COALESCE(string_agg(DISTINCT u.display_name,', ') FILTER(WHERE m.role='TESTER'),'') tester_names,(array_agg(DISTINCT m.user_id) FILTER(WHERE m.role='OWNER'))[1] owner_id,COALESCE(array_agg(DISTINCT m.user_id) FILTER(WHERE m.role='DEVELOPER'),'{}') developer_ids,COALESCE(array_agg(DISTINCT m.user_id) FILTER(WHERE m.role='TESTER'),'{}') tester_ids,count(DISTINCT i.id) instance_count,c.created_at,c.updated_at FROM gd_components c LEFT JOIN gd_component_members m ON m.component_id=c.id LEFT JOIN gd_users u ON u.id=m.user_id LEFT JOIN gd_component_instances i ON i.component_id=c.id";
const ENV_SELECT: &str = "SELECT e.id,e.customer_id,c.code customer_code,c.name customer_name,e.deployment_domain,e.code,e.name,e.jenkins_url,e.request_timeout_seconds,e.notes,e.is_active,e.version,e.created_at,e.updated_at FROM gd_environments e JOIN gd_customers c ON c.id=e.customer_id";
const INSTANCE_SELECT: &str = "SELECT i.id,i.name,i.component_id,comp.name component_name,i.environment_id,e.name environment_name,e.customer_id,cu.name customer_name,e.deployment_domain,i.folder_full_name,i.folder_url,i.status,i.notes,i.custom_fields,i.version,i.created_at,i.updated_at FROM gd_component_instances i JOIN gd_components comp ON comp.id=i.component_id JOIN gd_environments e ON e.id=i.environment_id JOIN gd_customers cu ON cu.id=e.customer_id";
const JOB_SELECT: &str = "SELECT j.id,j.component_instance_id,i.name instance_name,i.component_id,c.name component_name,i.environment_id,e.name environment_name,e.customer_id,cu.name customer_name,e.deployment_domain,j.display_name,j.description,j.job_full_name,j.job_url,j.status,j.current_version,j.version,v.parameter_definitions,v.parameter_presets,j.created_at,j.updated_at FROM gd_job_configs j JOIN gd_component_instances i ON i.id=j.component_instance_id JOIN gd_components c ON c.id=i.component_id JOIN gd_environments e ON e.id=i.environment_id JOIN gd_customers cu ON cu.id=e.customer_id JOIN gd_job_config_versions v ON v.job_config_id=j.id AND v.version=j.current_version";

#[derive(Deserialize)]
pub struct CustomerInput {
    code: String,
    name: String,
    #[serde(default = "yes")]
    is_active: bool,
    version: Option<i32>,
}
#[derive(Deserialize)]
pub struct ComponentInput {
    code: String,
    name: String,
    owner_id: Uuid,
    #[serde(default)]
    is_public: bool,
    #[serde(default = "yes")]
    is_active: bool,
    version: Option<i32>,
}
#[derive(Deserialize)]
pub struct EnvironmentInput {
    customer_id: Uuid,
    #[serde(default)]
    deployment_domain: String,
    code: String,
    jenkins_url: String,
    #[serde(default = "timeout")]
    request_timeout_seconds: i32,
    #[serde(default)]
    notes: String,
    version: Option<i32>,
}
#[derive(Deserialize)]
pub struct InstanceInput {
    name: String,
    component_id: Uuid,
    environment_id: Uuid,
    folder_full_name: String,
    folder_url: Option<String>,
    #[serde(default)]
    notes: String,
    #[serde(default = "empty_array")]
    custom_fields: Value,
}
#[derive(Deserialize)]
pub struct ComponentMembersInput {
    user_ids: Vec<Uuid>,
}
#[derive(Deserialize)]
pub struct JobInput {
    component_instance_id: Uuid,
    display_name: String,
    #[serde(default)]
    description: String,
    job_full_name: String,
    job_url: Option<String>,
    parameter_definitions: Value,
    #[serde(default = "empty_object")]
    parameter_presets: Value,
    version: Option<i32>,
}
#[derive(Deserialize)]
pub struct InstanceJobInput {
    job_full_name: String,
}
#[derive(Deserialize)]
pub struct InstanceJobTestInput {
    job_full_name: String,
    #[serde(default = "empty_object")]
    parameters: Value,
}
#[derive(Deserialize)]
pub struct InstanceJobQueueInput {
    queue_id: i64,
}
fn yes() -> bool {
    true
}
fn timeout() -> i32 {
    10
}
fn environment_name(code: &str) -> ApiResult<&'static str> {
    match code.trim().to_ascii_lowercase().as_str() {
        "dev" => Ok("开发"),
        "test" => Ok("测试"),
        "uat" => Ok("验收"),
        "branch" => Ok("分支"),
        "prod" => Ok("生产"),
        _ => Err(ApiError::bad_request(
            "INVALID_ENVIRONMENT_CODE",
            "环境代码必须为 dev、test、uat、branch 或 prod",
        )),
    }
}
fn validate_request_timeout(seconds: i32) -> ApiResult<()> {
    if !(1..=300).contains(&seconds) {
        return Err(ApiError::bad_request(
            "INVALID_REQUEST_TIMEOUT",
            "请求超时时间必须在 1 到 300 秒之间",
        ));
    }
    Ok(())
}
fn empty_array() -> Value {
    json!([])
}
fn empty_object() -> Value {
    json!({})
}

pub async fn customers(
    State(s): State<AppState>,
    _: CurrentUser,
    Query(p): Query<PageQuery>,
) -> ApiResult<Json<Page<CustomerView>>> {
    let pat = p.pattern();
    let total = sqlx::query_scalar(
        "SELECT count(*) FROM gd_customers WHERE code ILIKE $1 OR name ILIKE $1",
    )
    .bind(&pat)
    .fetch_one(&s.db)
    .await?;
    let items=sqlx::query_as::<_,CustomerView>("SELECT c.id,c.code,c.name,c.is_active,c.version,count(e.id) environment_count,c.created_at,c.updated_at FROM gd_customers c LEFT JOIN gd_environments e ON e.customer_id=c.id WHERE c.code ILIKE $1 OR c.name ILIKE $1 GROUP BY c.id ORDER BY c.updated_at DESC,c.id LIMIT $2 OFFSET $3").bind(pat).bind(p.limit()).bind(p.offset()).fetch_all(&s.db).await?;
    Ok(Json(Page {
        items,
        page: p.page.max(1),
        page_size: p.limit(),
        total,
    }))
}
pub async fn create_customer(
    State(s): State<AppState>,
    u: CurrentUser,
    Json(i): Json<CustomerInput>,
) -> ApiResult<Json<CustomerView>> {
    u.require_resource_maintainer()?;
    valid_code(&i.code)?;
    let r=sqlx::query_as::<_,CustomerView>("WITH x AS(INSERT INTO gd_customers(code,name,is_active) VALUES($1,$2,$3) RETURNING *) SELECT x.id,x.code,x.name,x.is_active,x.version,0::bigint environment_count,x.created_at,x.updated_at FROM x").bind(i.code.trim()).bind(i.name.trim()).bind(i.is_active).fetch_one(&s.db).await?;
    audit::record(
        &s.db,
        Some(u.id),
        "CREATE",
        "CUSTOMER",
        Some(r.id),
        json!({"code":r.code}),
    )
    .await;
    Ok(Json(r))
}
pub async fn update_customer(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
    Json(i): Json<CustomerInput>,
) -> ApiResult<Json<CustomerView>> {
    u.require_resource_maintainer()?;
    let v = i
        .version
        .ok_or_else(|| ApiError::bad_request("VERSION_REQUIRED", "缺少版本号"))?;
    let r=sqlx::query_as::<_,CustomerView>("WITH x AS(UPDATE gd_customers SET name=$2,is_active=$3,version=version+1,updated_at=now() WHERE id=$1 AND version=$4 RETURNING *) SELECT x.id,x.code,x.name,x.is_active,x.version,(SELECT count(*) FROM gd_environments WHERE customer_id=x.id) environment_count,x.created_at,x.updated_at FROM x").bind(id).bind(i.name.trim()).bind(i.is_active).bind(v).fetch_optional(&s.db).await?.ok_or_else(||ApiError::conflict("客户已被修改"))?;
    audit::record(
        &s.db,
        Some(u.id),
        "UPDATE",
        "CUSTOMER",
        Some(id),
        json!({"active":r.is_active}),
    )
    .await;
    Ok(Json(r))
}

pub async fn components(
    State(s): State<AppState>,
    u: CurrentUser,
    Query(p): Query<PageQuery>,
) -> ApiResult<Json<Page<ComponentView>>> {
    let pat = p.pattern();
    let allowed = if u.is_admin { None } else { Some(u.id) };
    let total:i64=sqlx::query_scalar("SELECT count(DISTINCT c.id) FROM gd_components c LEFT JOIN gd_component_members cm ON cm.component_id=c.id WHERE (c.code ILIKE $1 OR c.name ILIKE $1) AND ($2::uuid IS NULL OR c.is_public OR cm.user_id=$2)").bind(&pat).bind(allowed).fetch_one(&s.db).await?;
    let q = format!(
        "{COMPONENT_SELECT} WHERE (c.code ILIKE $1 OR c.name ILIKE $1) AND ($2::uuid IS NULL OR c.is_public OR EXISTS(SELECT 1 FROM gd_component_members mx WHERE mx.component_id=c.id AND mx.user_id=$2)) GROUP BY c.id ORDER BY c.updated_at DESC,c.id LIMIT $3 OFFSET $4"
    );
    let items = sqlx::query_as::<_, ComponentView>(&q)
        .bind(pat)
        .bind(allowed)
        .bind(p.limit())
        .bind(p.offset())
        .fetch_all(&s.db)
        .await?;
    Ok(Json(Page {
        items,
        page: p.page.max(1),
        page_size: p.limit(),
        total,
    }))
}
pub async fn create_component(
    State(s): State<AppState>,
    u: CurrentUser,
    Json(i): Json<ComponentInput>,
) -> ApiResult<Json<ComponentView>> {
    u.require_resource_maintainer()?;
    valid_code(&i.code)?;
    let mut tx = s.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO gd_components(code,name,is_public,is_active) VALUES($1,$2,$3,$4) RETURNING id",
    )
    .bind(i.code.trim())
    .bind(i.name.trim())
    .bind(i.is_public)
    .bind(i.is_active)
    .fetch_one(&mut *tx)
    .await?;
    replace_owner(&mut tx, id, i.owner_id).await?;
    tx.commit().await?;
    audit::record(
        &s.db,
        Some(u.id),
        "CREATE",
        "COMPONENT",
        Some(id),
        json!({"code":i.code}),
    )
    .await;
    Ok(Json(component_by_id(&s, id).await?))
}
pub async fn update_component(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
    Json(i): Json<ComponentInput>,
) -> ApiResult<Json<ComponentView>> {
    u.require_resource_maintainer()?;
    component_permission(&s.db, &u, id, true).await?;
    let v = i
        .version
        .ok_or_else(|| ApiError::bad_request("VERSION_REQUIRED", "缺少版本号"))?;
    let mut tx = s.db.begin().await?;
    if sqlx::query("UPDATE gd_components SET name=$2,is_public=$3,is_active=$4,version=version+1,updated_at=now() WHERE id=$1 AND version=$5").bind(id).bind(i.name.trim()).bind(i.is_public).bind(i.is_active).bind(v).execute(&mut *tx).await?.rows_affected()==0{return Err(ApiError::conflict("组件已被修改"));}
    replace_owner(&mut tx, id, i.owner_id).await?;
    tx.commit().await?;
    audit::record(
        &s.db,
        Some(u.id),
        "UPDATE",
        "COMPONENT",
        Some(id),
        json!({}),
    )
    .await;
    Ok(Json(component_by_id(&s, id).await?))
}

async fn replace_owner(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    owner_id: Uuid,
) -> ApiResult<()> {
    sqlx::query("DELETE FROM gd_component_members WHERE component_id=$1 AND role='OWNER'")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query(
        "INSERT INTO gd_component_members(component_id,user_id,role) VALUES($1,$2,'OWNER')",
    )
    .bind(id)
    .bind(owner_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn update_component_members(
    State(s): State<AppState>,
    u: CurrentUser,
    Path((id, role)): Path<(Uuid, String)>,
    Json(input): Json<ComponentMembersInput>,
) -> ApiResult<Json<ComponentView>> {
    u.require_resource_maintainer()?;
    component_permission(&s.db, &u, id, true).await?;
    let is_public: bool = sqlx::query_scalar("SELECT is_public FROM gd_components WHERE id=$1")
        .bind(id)
        .fetch_one(&s.db)
        .await?;
    if is_public {
        return Err(ApiError::bad_request(
            "PUBLIC_COMPONENT",
            "公共组件无需关联开发或测试人员",
        ));
    }
    let role = role.to_ascii_uppercase();
    if !matches!(role.as_str(), "DEVELOPER" | "TESTER") {
        return Err(ApiError::bad_request(
            "INVALID_COMPONENT_ROLE",
            "只能维护开发或测试人员",
        ));
    }
    let mut tx = s.db.begin().await?;
    sqlx::query("DELETE FROM gd_component_members WHERE component_id=$1 AND role=$2")
        .bind(id)
        .bind(&role)
        .execute(&mut *tx)
        .await?;
    for user_id in input.user_ids {
        sqlx::query("INSERT INTO gd_component_members(component_id,user_id,role) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(id).bind(user_id).bind(&role).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    audit::record(
        &s.db,
        Some(u.id),
        "ASSIGN",
        "COMPONENT_MEMBER",
        Some(id),
        json!({"role":role}),
    )
    .await;
    Ok(Json(component_by_id(&s, id).await?))
}
async fn component_by_id(s: &AppState, id: Uuid) -> ApiResult<ComponentView> {
    let q = format!("{COMPONENT_SELECT} WHERE c.id=$1 GROUP BY c.id");
    sqlx::query_as(&q)
        .bind(id)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(|| ApiError::not_found("组件不存在"))
}

pub async fn environments(
    State(s): State<AppState>,
    _: CurrentUser,
    Query(query): Query<EnvironmentQuery>,
) -> ApiResult<Json<Page<EnvironmentView>>> {
    let pat = query.pattern();
    let domain = query.deployment_domain.trim();
    let total=sqlx::query_scalar("SELECT count(*) FROM gd_environments e JOIN gd_customers c ON c.id=e.customer_id WHERE (e.code ILIKE $1 OR e.name ILIKE $1 OR e.deployment_domain ILIKE $1 OR c.name ILIKE $1) AND ($2::uuid IS NULL OR e.customer_id=$2) AND ($3='' OR e.deployment_domain=$3)").bind(&pat).bind(query.customer_id).bind(domain).fetch_one(&s.db).await?;
    let q = format!(
        "{ENV_SELECT} WHERE (e.code ILIKE $1 OR e.name ILIKE $1 OR e.deployment_domain ILIKE $1 OR c.name ILIKE $1) AND ($2::uuid IS NULL OR e.customer_id=$2) AND ($3='' OR e.deployment_domain=$3) ORDER BY e.updated_at DESC,e.id LIMIT $4 OFFSET $5"
    );
    let items = sqlx::query_as(&q)
        .bind(pat)
        .bind(query.customer_id)
        .bind(domain)
        .bind(query.limit())
        .bind(query.offset())
        .fetch_all(&s.db)
        .await?;
    Ok(Json(Page {
        items,
        page: query.page.max(1),
        page_size: query.limit(),
        total,
    }))
}
pub async fn create_environment(
    State(s): State<AppState>,
    u: CurrentUser,
    Json(mut i): Json<EnvironmentInput>,
) -> ApiResult<Json<EnvironmentView>> {
    u.require_resource_maintainer()?;
    valid_code(&i.code)?;
    validate_request_timeout(i.request_timeout_seconds)?;
    let name = environment_name(&i.code)?;
    i.deployment_domain = i.deployment_domain.trim().to_owned();
    let url = s
        .jenkins
        .validate_url(i.jenkins_url.trim_end_matches('/'))
        .await
        .map_err(|e| ApiError::bad_request("INVALID_JENKINS_URL", e.to_string()))?;
    let tested = s
        .jenkins
        .test(url.as_str(), i.request_timeout_seconds as u64, false)
        .await;
    let (active, test_error) = match tested {
        Ok(()) => (true, None),
        Err(e) => (false, Some(e.to_string())),
    };
    let id:Uuid=sqlx::query_scalar("INSERT INTO gd_environments(customer_id,deployment_domain,code,name,jenkins_url,request_timeout_seconds,notes,is_active) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id").bind(i.customer_id).bind(&i.deployment_domain).bind(i.code.trim().to_ascii_lowercase()).bind(name).bind(url.as_str().trim_end_matches('/')).bind(i.request_timeout_seconds).bind(i.notes).bind(active).fetch_one(&s.db).await?;
    audit::record(
        &s.db,
        Some(u.id),
        "CREATE",
        "ENVIRONMENT",
        Some(id),
        json!({"connection_ok":active,"test_error":test_error}),
    )
    .await;
    Ok(Json(environment_by_id(&s, id).await?))
}
pub async fn update_environment(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
    Json(mut i): Json<EnvironmentInput>,
) -> ApiResult<Json<EnvironmentView>> {
    u.require_resource_maintainer()?;
    let v = i
        .version
        .ok_or_else(|| ApiError::bad_request("VERSION_REQUIRED", "缺少版本号"))?;
    validate_request_timeout(i.request_timeout_seconds)?;
    let name = environment_name(&i.code)?;
    i.deployment_domain = i.deployment_domain.trim().to_owned();
    let bound: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM gd_component_instances WHERE environment_id=$1)",
    )
    .bind(id)
    .fetch_one(&s.db)
    .await?;
    if bound {
        let same: bool = sqlx::query_scalar(
            "SELECT customer_id=$2 AND deployment_domain=$3 FROM gd_environments WHERE id=$1",
        )
        .bind(id)
        .bind(i.customer_id)
        .bind(&i.deployment_domain)
        .fetch_one(&s.db)
        .await?;
        if !same {
            return Err(ApiError::conflict("已有组件实例时不能修改客户或部署域"));
        }
    }
    let url = s
        .jenkins
        .validate_url(i.jenkins_url.trim_end_matches('/'))
        .await
        .map_err(|e| ApiError::bad_request("INVALID_JENKINS_URL", e.to_string()))?;
    if sqlx::query("UPDATE gd_environments SET customer_id=$2,deployment_domain=$3,name=$4,jenkins_url=$5,request_timeout_seconds=$6,notes=$7,version=version+1,updated_at=now() WHERE id=$1 AND version=$8").bind(id).bind(i.customer_id).bind(&i.deployment_domain).bind(name).bind(url.as_str().trim_end_matches('/')).bind(i.request_timeout_seconds).bind(i.notes).bind(v).execute(&s.db).await?.rows_affected()==0{return Err(ApiError::conflict("环境已被修改"));}
    audit::record(
        &s.db,
        Some(u.id),
        "UPDATE",
        "ENVIRONMENT",
        Some(id),
        json!({}),
    )
    .await;
    Ok(Json(environment_by_id(&s, id).await?))
}
pub async fn test_environment(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<EnvironmentView>> {
    u.require_resource_maintainer()?;
    let e = environment_by_id(&s, id).await?;
    let result = s
        .jenkins
        .test(&e.jenkins_url, e.request_timeout_seconds as u64, false)
        .await;
    let (active, error) = match result {
        Ok(()) => (true, None),
        Err(e) => (false, Some(e.to_string())),
    };
    sqlx::query("UPDATE gd_environments SET is_active=$2,updated_at=now() WHERE id=$1")
        .bind(id)
        .bind(active)
        .execute(&s.db)
        .await?;
    audit::record(
        &s.db,
        Some(u.id),
        "CONNECTION_TEST",
        "ENVIRONMENT",
        Some(id),
        json!({"connection_ok":active,"error":error}),
    )
    .await;
    Ok(Json(environment_by_id(&s, id).await?))
}
pub async fn discover_folders(
    State(s): State<AppState>,
    _: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let e = environment_by_id(&s, id).await?;
    let jobs = s
        .jenkins
        .folders(&e.jenkins_url, e.request_timeout_seconds as u64, false)
        .await
        .map_err(|x| ApiError::bad_request("JENKINS_ERROR", x.to_string()))?;
    Ok(Json(json!(jobs)))
}
async fn environment_by_id(s: &AppState, id: Uuid) -> ApiResult<EnvironmentView> {
    let q = format!("{ENV_SELECT} WHERE e.id=$1");
    sqlx::query_as(&q)
        .bind(id)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(|| ApiError::not_found("环境不存在"))
}

pub async fn instances(
    State(s): State<AppState>,
    u: CurrentUser,
    Query(p): Query<InstanceQuery>,
) -> ApiResult<Json<Page<InstanceView>>> {
    let pat = p.pattern();
    let uid = (!u.is_admin).then_some(u.id);
    let domain = p.deployment_domain.trim();
    let total=sqlx::query_scalar("SELECT count(*) FROM gd_component_instances i JOIN gd_components c ON c.id=i.component_id JOIN gd_environments e ON e.id=i.environment_id WHERE (i.name ILIKE $1 OR i.folder_full_name ILIKE $1) AND ($2::uuid IS NULL OR c.is_public OR EXISTS(SELECT 1 FROM gd_component_members m WHERE m.component_id=i.component_id AND m.user_id=$2)) AND ($3::uuid IS NULL OR i.component_id=$3) AND ($4::uuid IS NULL OR e.customer_id=$4) AND ($5::uuid IS NULL OR i.environment_id=$5) AND ($6='' OR e.deployment_domain ILIKE '%' || $6 || '%')").bind(&pat).bind(uid).bind(p.component_id).bind(p.customer_id).bind(p.environment_id).bind(domain).fetch_one(&s.db).await?;
    let q = format!(
        "{INSTANCE_SELECT} WHERE (i.name ILIKE $1 OR i.folder_full_name ILIKE $1) AND ($2::uuid IS NULL OR comp.is_public OR EXISTS(SELECT 1 FROM gd_component_members m WHERE m.component_id=i.component_id AND m.user_id=$2)) AND ($3::uuid IS NULL OR i.component_id=$3) AND ($4::uuid IS NULL OR e.customer_id=$4) AND ($5::uuid IS NULL OR i.environment_id=$5) AND ($6='' OR e.deployment_domain ILIKE '%' || $6 || '%') ORDER BY i.updated_at DESC LIMIT $7 OFFSET $8"
    );
    let items = sqlx::query_as(&q)
        .bind(pat)
        .bind(uid)
        .bind(p.component_id)
        .bind(p.customer_id)
        .bind(p.environment_id)
        .bind(domain)
        .bind(p.limit())
        .bind(p.offset())
        .fetch_all(&s.db)
        .await?;
    Ok(Json(Page {
        items,
        page: p.page.max(1),
        page_size: p.limit(),
        total,
    }))
}
pub async fn create_instance(
    State(s): State<AppState>,
    u: CurrentUser,
    Json(i): Json<InstanceInput>,
) -> ApiResult<Json<InstanceView>> {
    u.require_resource_maintainer()?;
    let id:Uuid=sqlx::query_scalar("INSERT INTO gd_component_instances(name,component_id,environment_id,folder_full_name,folder_url,notes,custom_fields) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id").bind(i.name.trim()).bind(i.component_id).bind(i.environment_id).bind(i.folder_full_name.trim()).bind(i.folder_url).bind(i.notes).bind(i.custom_fields).fetch_one(&s.db).await?;
    audit::record(
        &s.db,
        Some(u.id),
        "BIND",
        "COMPONENT_INSTANCE",
        Some(id),
        json!({}),
    )
    .await;
    Ok(Json(instance_by_id(&s, id).await?))
}
pub async fn delete_instance(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::http::StatusCode> {
    let instance = instance_by_id(&s, id).await?;
    u.require_resource_maintainer()?;
    let used: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM gd_job_configs j WHERE j.component_instance_id=$1 AND (EXISTS(SELECT 1 FROM gd_node_executions n WHERE n.job_config_id=j.id) OR EXISTS(SELECT 1 FROM gd_task_versions t WHERE t.definition::text LIKE '%' || j.id::text || '%')))",
    )
    .bind(id)
    .fetch_one(&s.db)
    .await?;
    if used {
        return Err(ApiError::conflict(
            "该组件实例的 Job 已被任务或执行记录使用，无法删除",
        ));
    }
    let mut tx = s.db.begin().await?;
    sqlx::query("DELETE FROM gd_job_config_versions WHERE job_config_id IN(SELECT id FROM gd_job_configs WHERE component_instance_id=$1)")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM gd_job_configs WHERE component_instance_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM gd_component_instances WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    audit::record(
        &s.db,
        Some(u.id),
        "DELETE",
        "COMPONENT_INSTANCE",
        Some(id),
        json!({"name":instance.name}),
    )
    .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
async fn instance_by_id(s: &AppState, id: Uuid) -> ApiResult<InstanceView> {
    let q = format!("{INSTANCE_SELECT} WHERE i.id=$1");
    sqlx::query_as(&q)
        .bind(id)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(|| ApiError::not_found("组件实例不存在"))
}

pub async fn jobs(
    State(s): State<AppState>,
    u: CurrentUser,
    Query(p): Query<PageQuery>,
) -> ApiResult<Json<Page<JobConfigView>>> {
    let pat = p.pattern();
    let uid = (!u.is_admin).then_some(u.id);
    let total=sqlx::query_scalar("SELECT count(*) FROM gd_job_configs j WHERE (j.display_name ILIKE $1 OR j.job_full_name ILIKE $1) AND ($2::uuid IS NULL OR j.user_id=$2)").bind(&pat).bind(uid).fetch_one(&s.db).await?;
    let q = format!(
        "{JOB_SELECT} WHERE (j.display_name ILIKE $1 OR j.job_full_name ILIKE $1) AND ($2::uuid IS NULL OR j.user_id=$2) ORDER BY j.updated_at DESC LIMIT $3 OFFSET $4"
    );
    let items = sqlx::query_as(&q)
        .bind(pat)
        .bind(uid)
        .bind(p.limit())
        .bind(p.offset())
        .fetch_all(&s.db)
        .await?
        .into_iter()
        .map(mask_job)
        .collect();
    Ok(Json(Page {
        items,
        page: p.page.max(1),
        page_size: p.limit(),
        total,
    }))
}
pub async fn discover_jobs(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let inst = instance_by_id(&s, id).await?;
    component_permission(&s.db, &u, inst.component_id, false).await?;
    let e = environment_by_id(&s, inst.environment_id).await?;
    let folder_url = inst.folder_url.as_deref().ok_or_else(|| {
        ApiError::bad_request("FOLDER_URL_MISSING", "组件实例未记录 Jenkins Folder URL")
    })?;
    let jobs = s
        .jenkins
        .workflow_jobs(folder_url, e.request_timeout_seconds as u64, false)
        .await
        .map_err(|x| ApiError::bad_request("JENKINS_ERROR", x.to_string()))?;
    Ok(Json(json!(jobs)))
}

async fn instance_job(
    s: &AppState,
    u: &CurrentUser,
    instance_id: Uuid,
    full_name: &str,
) -> ApiResult<(crate::jenkins::JenkinsItem, EnvironmentView)> {
    let instance = instance_by_id(s, instance_id).await?;
    component_permission(&s.db, u, instance.component_id, false).await?;
    let environment = environment_by_id(s, instance.environment_id).await?;
    let folder_url = instance.folder_url.as_deref().ok_or_else(|| {
        ApiError::bad_request("FOLDER_URL_MISSING", "组件实例未记录 Jenkins Folder URL")
    })?;
    let jobs = s
        .jenkins
        .workflow_jobs(
            folder_url,
            environment.request_timeout_seconds as u64,
            false,
        )
        .await
        .map_err(|error| ApiError::bad_request("JENKINS_ERROR", error.to_string()))?;
    let job = jobs
        .into_iter()
        .find(|job| job.full_name == full_name)
        .ok_or_else(|| ApiError::not_found("该 WorkflowJob 不属于当前组件实例"))?;
    Ok((job, environment))
}

pub async fn preview_instance_job(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<InstanceJobInput>,
) -> ApiResult<Json<Value>> {
    let (job, environment) = instance_job(&s, &u, id, input.job_full_name.trim()).await?;
    let raw = s
        .jenkins
        .job_definition_at(&job.url, environment.request_timeout_seconds as u64, false)
        .await
        .map_err(|error| ApiError::bad_request("JENKINS_ERROR", error.to_string()))?;
    Ok(Json(json!({
        "job": job,
        "parameter_definitions": extract_parameter_definitions(&raw)
    })))
}

pub async fn test_instance_job(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<InstanceJobTestInput>,
) -> ApiResult<Json<Value>> {
    let instance = instance_by_id(&s, id).await?;
    component_permission(&s.db, &u, instance.component_id, false).await?;
    if !input.parameters.is_object() {
        return Err(ApiError::bad_request(
            "INVALID_PARAMETERS",
            "Job 参数必须是对象",
        ));
    }
    let (job, environment) = instance_job(&s, &u, id, input.job_full_name.trim()).await?;
    let location = s
        .jenkins
        .trigger_at(
            &job.url,
            &environment.jenkins_url,
            &input.parameters,
            environment.request_timeout_seconds as u64,
            false,
        )
        .await
        .map_err(|error| ApiError::bad_request("JENKINS_ERROR", error.to_string()))?;
    audit::record(
        &s.db,
        Some(u.id),
        "TEST_TRIGGER",
        "JENKINS_JOB",
        None,
        json!({"component_instance_id":id,"job_full_name":job.full_name}),
    )
    .await;
    let queue_id = location
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .and_then(|value| value.parse::<i64>().ok())
        .ok_or_else(|| {
            ApiError::bad_request(
                "INVALID_QUEUE_LOCATION",
                "无法从 Jenkins Location 解析队列 ID",
            )
        })?;
    Ok(Json(json!({"location": location,"queue_id":queue_id})))
}

pub async fn test_instance_job_queue(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
    Json(input): Json<InstanceJobQueueInput>,
) -> ApiResult<Json<Value>> {
    let instance = instance_by_id(&s, id).await?;
    component_permission(&s.db, &u, instance.component_id, false).await?;
    let environment = environment_by_id(&s, instance.environment_id).await?;
    let queue = s
        .jenkins
        .queue(
            &environment.jenkins_url,
            input.queue_id,
            environment.request_timeout_seconds as u64,
            false,
        )
        .await
        .map_err(|error| ApiError::bad_request("JENKINS_ERROR", error.to_string()))?;
    Ok(Json(json!({
        "cancelled":queue.cancelled,
        "why":queue.why,
        "executable_url":queue.executable.as_ref().map(|item| item.url.clone()),
        "build_number":queue.executable.as_ref().map(|item| item.number)
    })))
}
pub async fn create_job(
    State(s): State<AppState>,
    u: CurrentUser,
    Json(i): Json<JobInput>,
) -> ApiResult<Json<JobConfigView>> {
    let inst = instance_by_id(&s, i.component_instance_id).await?;
    component_permission(&s.db, &u, inst.component_id, false).await?;
    validate_definitions(&i.parameter_definitions)?;
    let presets = s
        .crypto
        .encrypt_parameters(
            &i.parameter_presets,
            &crate::crypto::password_names(&i.parameter_definitions),
        )
        .map_err(ApiError::Internal)?;
    let mut tx = s.db.begin().await?;
    let id:Uuid=sqlx::query_scalar("INSERT INTO gd_job_configs(user_id,component_instance_id,display_name,description,job_full_name,job_url) VALUES($1,$2,$3,$4,$5,$6) RETURNING id").bind(u.id).bind(i.component_instance_id).bind(i.display_name.trim()).bind(i.description).bind(i.job_full_name.trim()).bind(i.job_url).fetch_one(&mut *tx).await?;
    let hash = definition_hash(&i.parameter_definitions);
    sqlx::query("INSERT INTO gd_job_config_versions(job_config_id,version,parameter_definitions,parameter_presets,definition_hash,created_by) VALUES($1,1,$2,$3,$4,$5)").bind(id).bind(i.parameter_definitions).bind(presets).bind(hash).bind(u.id).execute(&mut *tx).await?;
    tx.commit().await?;
    audit::record(
        &s.db,
        Some(u.id),
        "CREATE",
        "JOB_CONFIG",
        Some(id),
        json!({}),
    )
    .await;
    Ok(Json(mask_job(job_by_id(&s, id).await?)))
}
pub async fn update_job(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
    Json(i): Json<JobInput>,
) -> ApiResult<Json<JobConfigView>> {
    let _existing = job_by_id_for_user(&s, &u, id).await?;
    validate_definitions(&i.parameter_definitions)?;
    let presets = s
        .crypto
        .encrypt_parameters(
            &i.parameter_presets,
            &crate::crypto::password_names(&i.parameter_definitions),
        )
        .map_err(ApiError::Internal)?;
    let v = i
        .version
        .ok_or_else(|| ApiError::bad_request("VERSION_REQUIRED", "缺少版本号"))?;
    let mut tx = s.db.begin().await?;
    let next:i32=sqlx::query_scalar("UPDATE gd_job_configs SET display_name=$2,description=$3,status='ACTIVE',current_version=current_version+1,version=version+1,updated_at=now() WHERE id=$1 AND version=$4 RETURNING current_version").bind(id).bind(i.display_name.trim()).bind(i.description).bind(v).fetch_optional(&mut *tx).await?.ok_or_else(||ApiError::conflict("Job 配置已被修改"))?;
    sqlx::query("INSERT INTO gd_job_config_versions(job_config_id,version,parameter_definitions,parameter_presets,definition_hash,created_by) VALUES($1,$2,$3,$4,$5,$6)").bind(id).bind(next).bind(&i.parameter_definitions).bind(presets).bind(definition_hash(&i.parameter_definitions)).bind(u.id).execute(&mut *tx).await?;
    tx.commit().await?;
    audit::record(
        &s.db,
        Some(u.id),
        "VERSION",
        "JOB_CONFIG",
        Some(id),
        json!({"version":next}),
    )
    .await;
    Ok(Json(mask_job(job_by_id(&s, id).await?)))
}
pub async fn sync_job(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let j = job_by_id_for_user(&s, &u, id).await?;
    let inst = instance_by_id(&s, j.component_instance_id).await?;
    let e = environment_by_id(&s, inst.environment_id).await?;
    let raw = s
        .jenkins
        .job_definition(
            &e.jenkins_url,
            &j.job_full_name,
            e.request_timeout_seconds as u64,
            false,
        )
        .await
        .map_err(|x| ApiError::bad_request("JENKINS_ERROR", x.to_string()))?;
    let latest = extract_parameter_definitions(&raw);
    let current = &j.parameter_definitions;
    let comparison = compare_definitions(current, &latest);
    if !comparison
        .get("compatible")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        sqlx::query("UPDATE gd_job_configs SET status='STALE',updated_at=now() WHERE id=$1")
            .bind(id)
            .execute(&s.db)
            .await?;
    }
    Ok(Json(
        json!({"comparison":comparison,"saved":current,"latest":latest}),
    ))
}
async fn job_by_id(s: &AppState, id: Uuid) -> ApiResult<JobConfigView> {
    let q = format!("{JOB_SELECT} WHERE j.id=$1");
    sqlx::query_as(&q)
        .bind(id)
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(|| ApiError::not_found("Job 配置不存在"))
}
async fn job_by_id_for_user(s: &AppState, u: &CurrentUser, id: Uuid) -> ApiResult<JobConfigView> {
    let q = format!("{JOB_SELECT} WHERE j.id=$1 AND ($2::uuid IS NULL OR j.user_id=$2)");
    sqlx::query_as(&q)
        .bind(id)
        .bind(if u.is_admin { None } else { Some(u.id) })
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(|| ApiError::not_found("Job 配置不存在"))
}

fn valid_code(code: &str) -> ApiResult<()> {
    if code.len() < 2
        || code.len() > 64
        || !code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Err(ApiError::bad_request(
            "INVALID_CODE",
            "代码仅允许 2-64 位字母、数字、连字符和下划线",
        ))
    } else {
        Ok(())
    }
}
fn definition_hash(value: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).unwrap_or_default())
    )
}

pub(crate) fn extract_parameter_definitions(raw: &Value) -> Value {
    let definitions = raw
        .get("property")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|property| {
            property
                .get("parameterDefinitions")
                .and_then(Value::as_array)
        })
        .cloned()
        .unwrap_or_default();
    Value::Array(definitions.into_iter().map(|item| {
        let class = item.get("_class").and_then(Value::as_str).unwrap_or("");
        let parameter_type = if class.contains("TextParameter") { "Text" }
            else if class.contains("ChoiceParameter") { "Choice" }
            else if class.contains("BooleanParameter") { "Boolean" }
            else if class.contains("PasswordParameter") { "Password" }
            else if class.contains("StringParameter") { "String" }
            else { class.rsplit('.').next().unwrap_or(class) };
        json!({
            "name": item.get("name").cloned().unwrap_or(Value::Null),
            "type": parameter_type,
            "description": item.get("description").cloned().unwrap_or(Value::Null),
            "default": item.get("defaultParameterValue").and_then(|value| value.get("value")).cloned().unwrap_or(Value::Null),
            "choices": item.get("choices").cloned().unwrap_or_else(|| json!([]))
        })
    }).collect())
}

pub(crate) fn compare_definitions(saved: &Value, latest: &Value) -> Value {
    fn by_name(value: &Value) -> std::collections::HashMap<String, &Value> {
        value
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|parameter| {
                parameter
                    .get("name")
                    .and_then(Value::as_str)
                    .map(|name| (name.to_owned(), parameter))
            })
            .collect()
    }
    let old = by_name(saved);
    let new = by_name(latest);
    let mut incompatible = Vec::new();
    let mut defaults = Vec::new();
    for (name, parameter) in &old {
        match new.get(name) {
            None => incompatible.push(format!("参数 {name} 已删除")),
            Some(next) => {
                let old_type = parameter
                    .get("type")
                    .or_else(|| parameter.get("_class"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let new_type = next.get("type").and_then(Value::as_str).unwrap_or("");
                if old_type != new_type && !old_type.ends_with(new_type) {
                    incompatible.push(format!("参数 {name} 类型由 {old_type} 变为 {new_type}"));
                }
                if new_type == "Choice" {
                    let choices = next
                        .get("choices")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    if let Some(value) = parameter
                        .get("default")
                        .or_else(|| parameter.get("default_value"))
                        && !value.is_null()
                        && !choices.contains(value)
                    {
                        incompatible.push(format!("参数 {name} 的原值已不在 Choice 选项中"));
                    }
                }
                let saved_default = parameter
                    .get("default")
                    .or_else(|| parameter.get("default_value"));
                if saved_default != next.get("default") {
                    defaults.push(
                        json!({"name":name,"saved":saved_default,"latest":next.get("default")}),
                    );
                }
            }
        }
    }
    for name in new.keys() {
        if !old.contains_key(name) {
            incompatible.push(format!("新增参数 {name}"));
        }
    }
    json!({"compatible":incompatible.is_empty(),"incompatible_changes":incompatible,"default_changes":defaults})
}
fn validate_definitions(value: &Value) -> ApiResult<()> {
    let supported = [
        "String",
        "Text",
        "Choice",
        "Boolean",
        "Password",
        "StringParameterDefinition",
        "TextParameterDefinition",
        "ChoiceParameterDefinition",
        "BooleanParameterDefinition",
        "PasswordParameterDefinition",
    ];
    for p in value
        .as_array()
        .ok_or_else(|| ApiError::bad_request("INVALID_PARAMETERS", "参数定义必须是数组"))?
    {
        let typ = p
            .get("type")
            .or_else(|| p.get("_class"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if !supported.iter().any(|x| typ.ends_with(x)) {
            return Err(ApiError::bad_request(
                "UNSUPPORTED_PARAMETER",
                format!("不支持的 Jenkins 参数类型：{typ}"),
            ));
        }
    }
    Ok(())
}
fn mask_job(mut job: JobConfigView) -> JobConfigView {
    crate::crypto::mask_encrypted(&mut job.parameter_presets);
    job
}

#[cfg(test)]
mod tests {
    use super::{compare_definitions, extract_parameter_definitions};
    use serde_json::json;

    #[test]
    fn extracts_supported_jenkins_parameters() {
        let raw = json!({"property":[{"parameterDefinitions":[{"_class":"hudson.model.ChoiceParameterDefinition","name":"ENV","choices":["test","prod"],"defaultParameterValue":{"value":"test"}}]}]});
        let definitions = extract_parameter_definitions(&raw);
        assert_eq!(definitions[0]["type"], "Choice");
        assert_eq!(definitions[0]["default"], "test");
    }

    #[test]
    fn rejects_added_and_changed_parameters() {
        let saved =
            json!([{"name":"ENV","type":"Choice","choices":["test","prod"],"default":"test"}]);
        let latest = json!([{"name":"ENV","type":"Choice","choices":["prod"],"default":"prod"},{"name":"VERSION","type":"String"}]);
        let result = compare_definitions(&saved, &latest);
        assert_eq!(result["compatible"], false);
        assert_eq!(result["incompatible_changes"].as_array().unwrap().len(), 2);
    }
}

use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};

use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::{
    app::AppState,
    audit,
    auth::CurrentUser,
    error::{ApiError, ApiResult},
    execution,
    models::{ExecutionView, NodeExecutionView, Page, PageQuery, TaskView},
};

const TASK_SELECT: &str = "SELECT t.id,t.name,t.description,t.creator_id,u.display_name creator_name,t.trigger_type,t.scheduled_at,t.cron_expression,t.timezone,t.is_enabled,t.current_version,t.version,t.next_run_at,t.pinned_at,v.definition,t.created_at,t.updated_at FROM gd_tasks t JOIN gd_users u ON u.id=t.creator_id JOIN gd_task_versions v ON v.task_id=t.id AND v.version=t.current_version";
const EXEC_SELECT: &str = "SELECT e.id,e.task_id,t.name task_name,e.task_version,e.trigger_type,e.status,e.snapshot,e.scheduled_at,e.started_at,e.finished_at,e.created_at FROM gd_executions e JOIN gd_tasks t ON t.id=e.task_id";

#[derive(Deserialize)]
pub struct TaskInput {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub trigger_type: String,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub cron_expression: Option<String>,
    #[serde(default = "tz")]
    pub timezone: String,
    #[serde(default = "yes")]
    pub is_enabled: bool,
    pub definition: Value,
    pub version: Option<i32>,
}
fn tz() -> String {
    "Asia/Shanghai".into()
}
fn yes() -> bool {
    true
}

pub async fn list(
    State(s): State<AppState>,
    u: CurrentUser,
    Query(p): Query<PageQuery>,
) -> ApiResult<Json<Page<TaskView>>> {
    let pat = p.pattern();
    let total = if u.is_admin {
        sqlx::query_scalar("SELECT count(*) FROM gd_tasks WHERE NOT is_deleted AND name ILIKE $1")
            .bind(&pat)
            .fetch_one(&s.db)
            .await?
    } else {
        sqlx::query_scalar(
            "SELECT count(*) FROM gd_tasks WHERE NOT is_deleted AND name ILIKE $1 AND user_id=$2",
        )
        .bind(&pat)
        .bind(u.id)
        .fetch_one(&s.db)
        .await?
    };
    let q = format!(
        "{TASK_SELECT} WHERE NOT t.is_deleted AND t.name ILIKE $1 AND ($2::uuid IS NULL OR t.user_id=$2) ORDER BY t.pinned_at IS NULL,t.pinned_at ASC,t.created_at DESC LIMIT $3 OFFSET $4"
    );
    let items = sqlx::query_as(&q)
        .bind(pat)
        .bind(if u.is_admin { None } else { Some(u.id) })
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
pub async fn get(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<TaskView>> {
    Ok(Json(task_by_id(&s, &u, id).await?))
}
pub async fn create(
    State(s): State<AppState>,
    u: CurrentUser,
    headers: HeaderMap,
    Json(i): Json<TaskInput>,
) -> ApiResult<Json<Value>> {
    validate_task(&s, &u, &i).await?;
    let definition = encrypt_definition(&s, &i.definition).await?;
    let (next, scheduled) = schedule_fields(&i)?;
    let mut tx = s.db.begin().await?;
    let id:Uuid=sqlx::query_scalar("INSERT INTO gd_tasks(user_id,name,description,creator_id,trigger_type,scheduled_at,cron_expression,timezone,is_enabled,next_run_at) VALUES($1,$2,$3,$1,$4,$5,$6,$7,$8,$9) RETURNING id").bind(u.id).bind(i.name.trim()).bind(i.description).bind(&i.trigger_type).bind(scheduled).bind(&i.cron_expression).bind(&i.timezone).bind(i.is_enabled).bind(next).fetch_one(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO gd_task_versions(task_id,version,definition,created_by) VALUES($1,1,$2,$3)",
    )
    .bind(id)
    .bind(&definition)
    .bind(u.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let execution_id = if i.trigger_type == "IMMEDIATE" {
        let key = headers
            .get("Idempotency-Key")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                ApiError::bad_request(
                    "IDEMPOTENCY_KEY_REQUIRED",
                    "立即执行必须提供 Idempotency-Key",
                )
            })?;
        Some(
            execution::create_execution(
                &s,
                id,
                1,
                format!("manual:{}:{key}", u.id),
                "IMMEDIATE",
                Some(u.id),
                None,
            )
            .await?,
        )
    } else {
        None
    };
    audit::record(
        &s.db,
        Some(u.id),
        "CREATE",
        "TASK",
        Some(id),
        json!({"trigger_type":i.trigger_type}),
    )
    .await;
    let mut response =
        serde_json::to_value(task_by_id(&s, &u, id).await?).map_err(anyhow::Error::from)?;
    response["execution_id"] = json!(execution_id);
    Ok(Json(response))
}
pub async fn update(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
    Json(i): Json<TaskInput>,
) -> ApiResult<Json<TaskView>> {
    let old = task_by_id(&s, &u, id).await?;
    if !u.is_admin && old.creator_id != u.id {
        return Err(ApiError::forbidden());
    }
    validate_task(&s, &u, &i).await?;
    let definition = encrypt_definition(&s, &i.definition).await?;
    let v = i
        .version
        .ok_or_else(|| ApiError::bad_request("VERSION_REQUIRED", "缺少版本号"))?;
    let (next, scheduled) = schedule_fields(&i)?;
    let mut tx = s.db.begin().await?;
    let nv:i32=sqlx::query_scalar("UPDATE gd_tasks SET name=$2,description=$3,trigger_type=$4,scheduled_at=$5,cron_expression=$6,timezone=$7,is_enabled=$8,next_run_at=$9,current_version=current_version+1,version=version+1,updated_at=now() WHERE id=$1 AND version=$10 RETURNING current_version").bind(id).bind(i.name.trim()).bind(i.description).bind(&i.trigger_type).bind(scheduled).bind(&i.cron_expression).bind(&i.timezone).bind(i.is_enabled).bind(next).bind(v).fetch_optional(&mut *tx).await?.ok_or_else(||ApiError::conflict("任务已被修改"))?;
    sqlx::query(
        "INSERT INTO gd_task_versions(task_id,version,definition,created_by) VALUES($1,$2,$3,$4)",
    )
    .bind(id)
    .bind(nv)
    .bind(&definition)
    .bind(u.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    audit::record(
        &s.db,
        Some(u.id),
        "UPDATE",
        "TASK",
        Some(id),
        json!({"version":nv}),
    )
    .await;
    Ok(Json(task_by_id(&s, &u, id).await?))
}
pub async fn toggle(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<TaskView>> {
    let t = task_by_id(&s, &u, id).await?;
    if !u.is_admin && t.creator_id != u.id {
        return Err(ApiError::forbidden());
    }
    sqlx::query(
        "UPDATE gd_tasks SET is_enabled=NOT is_enabled,version=version+1,updated_at=now() WHERE id=$1",
    )
    .bind(id)
    .execute(&s.db)
    .await?;
    audit::record(&s.db, Some(u.id), "TOGGLE", "TASK", Some(id), json!({})).await;
    Ok(Json(task_by_id(&s, &u, id).await?))
}
pub async fn toggle_pin(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<TaskView>> {
    let task = task_by_id(&s, &u, id).await?;
    if !u.is_admin && task.creator_id != u.id {
        return Err(ApiError::forbidden());
    }
    sqlx::query("UPDATE gd_tasks SET pinned_at=CASE WHEN pinned_at IS NULL THEN now() ELSE NULL END WHERE id=$1")
        .bind(id).execute(&s.db).await?;
    audit::record(&s.db, Some(u.id), "PIN", "TASK", Some(id), json!({})).await;
    Ok(Json(task_by_id(&s, &u, id).await?))
}
pub async fn delete_task(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::http::StatusCode> {
    let task = task_by_id(&s, &u, id).await?;
    if !u.is_admin && task.creator_id != u.id {
        return Err(ApiError::forbidden());
    }
    let running: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM gd_executions WHERE task_id=$1 AND status IN('SCHEDULED','RUNNING','CANCELING'))",
    )
    .bind(id)
    .fetch_one(&s.db)
    .await?;
    if running {
        return Err(ApiError::conflict("任务存在未结束的执行，暂时不能删除"));
    }
    sqlx::query("UPDATE gd_tasks SET is_deleted=true,is_enabled=false,next_run_at=NULL,version=version+1,updated_at=now() WHERE id=$1")
        .bind(id)
        .execute(&s.db)
        .await?;
    audit::record(
        &s.db,
        Some(u.id),
        "DELETE",
        "TASK",
        Some(id),
        json!({"name":task.name}),
    )
    .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
pub async fn run(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> ApiResult<Json<ExecutionView>> {
    let t = task_by_id(&s, &u, id).await?;
    validate_definition_permissions(&s, &u, &t.definition).await?;
    validate_password_parameters(&s, &t.definition).await?;
    let key = headers
        .get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::bad_request("IDEMPOTENCY_KEY_REQUIRED", "缺少 Idempotency-Key"))?;
    let eid = execution::create_execution(
        &s,
        id,
        t.current_version,
        format!("manual:{}:{key}", u.id),
        "IMMEDIATE",
        Some(u.id),
        None,
    )
    .await?;
    audit::record(
        &s.db,
        Some(u.id),
        "RUN",
        "TASK",
        Some(id),
        json!({"execution_id":eid}),
    )
    .await;
    Ok(Json(execution_by_id(&s, &u, eid).await?))
}
pub async fn preview(Json(input): Json<Value>) -> ApiResult<Json<Value>> {
    let expr = input
        .get("cron_expression")
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::bad_request("INVALID_CRON", "缺少 Cron 表达式"))?;
    let timezone = input
        .get("timezone")
        .and_then(Value::as_str)
        .unwrap_or("Asia/Shanghai");
    let values = next_times(expr, timezone, 5)?;
    Ok(Json(json!({"times":values})))
}

pub async fn executions(
    State(s): State<AppState>,
    u: CurrentUser,
    Query(p): Query<PageQuery>,
) -> ApiResult<Json<Page<ExecutionView>>> {
    let pat = p.pattern();
    let total = sqlx::query_scalar(
        "SELECT count(*) FROM gd_executions e JOIN gd_tasks t ON t.id=e.task_id WHERE t.name ILIKE $1 AND ($2::uuid IS NULL OR e.user_id=$2)",
    )
    .bind(&pat)
    .bind(if u.is_admin { None } else { Some(u.id) })
    .fetch_one(&s.db)
    .await?;
    let q = format!(
        "{EXEC_SELECT} WHERE t.name ILIKE $1 AND ($2::uuid IS NULL OR e.user_id=$2) ORDER BY e.created_at DESC LIMIT $3 OFFSET $4"
    );
    let items = sqlx::query_as(&q)
        .bind(pat)
        .bind(if u.is_admin { None } else { Some(u.id) })
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
pub async fn execution_detail(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let execution = execution_by_id(&s, &u, id).await?;
    let nodes=sqlx::query_as::<_,NodeExecutionView>("SELECT n.id,n.execution_id,n.node_key,n.node_name,c.name component_name,cu.name customer_name,e.deployment_domain,e.name environment_name,i.argo_url,i.wiki_url,i.apollo_url,i.log_url,n.dependencies,n.status,n.queue_id,n.queue_url,n.build_number,n.build_url,n.blocking_reason,n.error_summary,n.submitted_at,n.started_at,n.finished_at,n.updated_at FROM gd_node_executions n JOIN gd_job_configs j ON j.id=n.job_config_id JOIN gd_component_instances i ON i.id=j.component_instance_id JOIN gd_components c ON c.id=i.component_id JOIN gd_environments e ON e.id=n.environment_id JOIN gd_customers cu ON cu.id=e.customer_id WHERE n.execution_id=$1 ORDER BY n.submitted_at NULLS FIRST,n.node_name").bind(id).fetch_all(&s.db).await?;
    Ok(Json(json!({
        "execution": execution,
        "nodes": nodes,
        "worker_interval_seconds": s.config.worker_interval.as_secs(),
    })))
}
pub async fn delete_execution(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::http::StatusCode> {
    let status: String = sqlx::query_scalar(
        "SELECT status FROM gd_executions WHERE id=$1 AND ($2::uuid IS NULL OR user_id=$2)",
    )
    .bind(id)
    .bind(if u.is_admin { None } else { Some(u.id) })
    .fetch_optional(&s.db)
    .await?
    .ok_or_else(|| ApiError::not_found("执行记录不存在"))?;
    if !matches!(status.as_str(), "SUCCESS" | "FAILED" | "CANCELED") {
        return Err(ApiError::conflict("执行尚未结束，不能删除"));
    }
    let mut tx = s.db.begin().await?;
    sqlx::query("DELETE FROM gd_node_executions WHERE execution_id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM gd_executions WHERE id=$1")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    audit::record(
        &s.db,
        Some(u.id),
        "DELETE",
        "EXECUTION",
        Some(id),
        json!({"status":status}),
    )
    .await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}
pub async fn stop_execution(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let definition: Value = sqlx::query_scalar(
        "SELECT snapshot FROM gd_executions WHERE id=$1 AND ($2::uuid IS NULL OR user_id=$2)",
    )
    .bind(id)
    .bind(if u.is_admin { None } else { Some(u.id) })
    .fetch_optional(&s.db)
    .await?
    .ok_or_else(|| ApiError::not_found("执行记录不存在"))?;
    validate_definition_permissions(&s, &u, &definition).await?;
    execution::request_cancel(&s, id).await?;
    audit::record(&s.db, Some(u.id), "STOP", "EXECUTION", Some(id), json!({})).await;
    Ok(Json(json!({"ok":true,"status":"CANCELED"})))
}

pub async fn copy_execution(
    State(s): State<AppState>,
    u: CurrentUser,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Value>> {
    let (name, mut definition): (String, Value) = sqlx::query_as(
        "SELECT t.name,e.snapshot FROM gd_executions e JOIN gd_tasks t ON t.id=e.task_id WHERE e.id=$1 AND e.status IN('SUCCESS','FAILED','CANCELED') AND ($2::uuid IS NULL OR e.user_id=$2)",
    )
    .bind(id)
    .bind(if u.is_admin { None } else { Some(u.id) })
    .fetch_optional(&s.db)
    .await?
    .ok_or_else(|| ApiError::conflict("只有已结束执行才能复制"))?;
    validate_definition_permissions(&s, &u, &definition).await?;
    clear_encrypted(&mut definition);
    let task_id: Uuid = sqlx::query_scalar("INSERT INTO gd_tasks(user_id,name,description,creator_id,trigger_type,timezone,is_enabled) VALUES($2,$1,'从历史执行复制；敏感参数需要重新填写',$2,'IMMEDIATE','Asia/Shanghai',false) RETURNING id")
        .bind(format!("{name} - 复制"))
        .bind(u.id)
        .fetch_one(&s.db)
        .await?;
    sqlx::query(
        "INSERT INTO gd_task_versions(task_id,version,definition,created_by) VALUES($1,1,$2,$3)",
    )
    .bind(task_id)
    .bind(definition)
    .bind(u.id)
    .execute(&s.db)
    .await?;
    audit::record(
        &s.db,
        Some(u.id),
        "COPY",
        "EXECUTION",
        Some(id),
        json!({"task_id":task_id}),
    )
    .await;
    Ok(Json(json!({"task_id":task_id})))
}
pub async fn node_log(
    State(s): State<AppState>,
    u: CurrentUser,
    Path((eid, nid)): Path<(Uuid, Uuid)>,
) -> ApiResult<Json<Value>> {
    let row=sqlx::query_as::<_,(String,String,Option<String>,i32,bool,String,Option<i64>,i64)>("SELECT e.jenkins_url,j.job_full_name,j.job_url,e.request_timeout_seconds,false,n.status,n.build_number,n.log_offset FROM gd_node_executions n JOIN gd_executions x ON x.id=n.execution_id JOIN gd_environments e ON e.id=n.environment_id JOIN gd_job_configs j ON j.id=n.job_config_id WHERE n.id=$1 AND n.execution_id=$2 AND ($3::uuid IS NULL OR x.user_id=$3)").bind(nid).bind(eid).bind(if u.is_admin { None } else { Some(u.id) }).fetch_optional(&s.db).await?.ok_or_else(||ApiError::not_found("节点不存在"))?;
    let Some(number) = row.6 else {
        return Ok(Json(
            json!({"text":"","next_offset":row.7,"more":true,"reason":"构建尚未开始"}),
        ));
    };
    let log_result = if let Some(job_url) = row.2.as_deref() {
        s.jenkins
            .progressive_log_at(job_url, number, row.7, row.3 as u64, row.4)
            .await
    } else {
        s.jenkins
            .progressive_log(&row.0, &row.1, number, row.7, row.3 as u64, row.4)
            .await
    };
    let (text, next, more) =
        log_result.map_err(|e| ApiError::bad_request("LOG_UNAVAILABLE", e.to_string()))?;
    sqlx::query("UPDATE gd_node_executions SET log_offset=$2 WHERE id=$1 AND log_offset<$2")
        .bind(nid)
        .bind(next)
        .execute(&s.db)
        .await?;
    Ok(Json(
        json!({"text":redact(&text),"next_offset":next,"more":more}),
    ))
}

async fn task_by_id(s: &AppState, u: &CurrentUser, id: Uuid) -> ApiResult<TaskView> {
    let q = format!(
        "{TASK_SELECT} WHERE t.id=$1 AND NOT t.is_deleted AND ($2::uuid IS NULL OR t.user_id=$2)"
    );
    sqlx::query_as(&q)
        .bind(id)
        .bind(if u.is_admin { None } else { Some(u.id) })
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(|| ApiError::not_found("任务不存在"))
}
async fn execution_by_id(s: &AppState, u: &CurrentUser, id: Uuid) -> ApiResult<ExecutionView> {
    let q = format!("{EXEC_SELECT} WHERE e.id=$1 AND ($2::uuid IS NULL OR e.user_id=$2)");
    sqlx::query_as(&q)
        .bind(id)
        .bind(if u.is_admin { None } else { Some(u.id) })
        .fetch_optional(&s.db)
        .await?
        .ok_or_else(|| ApiError::not_found("执行记录不存在"))
}
async fn encrypt_definition(s: &AppState, definition: &Value) -> ApiResult<Value> {
    let mut output = definition.clone();
    normalize_level_dependencies(&mut output)?;
    let nodes = output
        .get_mut("nodes")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| ApiError::bad_request("EMPTY_TASK", "任务没有节点"))?;
    for node in nodes {
        let jid = parse_uuid(node.get("job_config_id"))?;
        let defs:Value=sqlx::query_scalar("SELECT v.parameter_definitions FROM gd_job_configs j JOIN gd_job_config_versions v ON v.job_config_id=j.id AND v.version=j.current_version WHERE j.id=$1").bind(jid).fetch_one(&s.db).await?;
        if let Some(parameters) = node.get_mut("parameters") {
            *parameters = s
                .crypto
                .encrypt_parameters(parameters, &crate::crypto::password_names(&defs))
                .map_err(ApiError::Internal)?;
        }
    }
    Ok(output)
}

pub(crate) fn normalize_level_dependencies(definition: &mut Value) -> ApiResult<()> {
    let nodes = definition
        .get_mut("nodes")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| ApiError::bad_request("EMPTY_TASK", "任务没有节点"))?;
    let node_data = nodes
        .iter()
        .map(|node| {
            let key = node
                .get("key")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let dependencies = node
                .get("dependencies")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            (key, dependencies)
        })
        .collect::<Vec<_>>();
    let known_keys = node_data
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<HashSet<_>>();
    let mut remaining = (0..node_data.len()).collect::<HashSet<_>>();
    let mut completed = HashSet::<String>::new();
    let mut levels = Vec::<Vec<usize>>::new();
    while !remaining.is_empty() {
        let mut level = remaining
            .iter()
            .copied()
            .filter(|index| {
                node_data[*index].1.iter().all(|dependency| {
                    completed.contains(dependency) || !known_keys.contains(dependency.as_str())
                })
            })
            .collect::<Vec<_>>();
        if level.is_empty() {
            return Err(ApiError::bad_request("DAG_CYCLE", "任务编排存在循环依赖"));
        }
        level.sort_unstable();
        for index in &level {
            remaining.remove(index);
            completed.insert(node_data[*index].0.clone());
        }
        levels.push(level);
    }
    for (level_index, level) in levels.iter().enumerate() {
        let dependencies = if level_index == 0 {
            Vec::new()
        } else {
            levels[level_index - 1]
                .iter()
                .map(|index| node_data[*index].0.clone())
                .collect::<Vec<_>>()
        };
        for index in level {
            nodes[*index]["dependencies"] = json!(dependencies);
        }
    }
    Ok(())
}
async fn validate_task(s: &AppState, u: &CurrentUser, i: &TaskInput) -> ApiResult<()> {
    if i.name.trim().is_empty() || i.name.len() > 200 {
        return Err(ApiError::bad_request(
            "INVALID_NAME",
            "任务名称不能为空且最多 200 字符",
        ));
    }
    match i.trigger_type.as_str() {
        "IMMEDIATE" => {}
        "ONCE" => {
            if i.scheduled_at.is_none_or(|x| x <= Utc::now()) {
                return Err(ApiError::bad_request(
                    "INVALID_SCHEDULE",
                    "指定时间必须晚于当前时间",
                ));
            }
        }
        "CRON" => {
            next_times(i.cron_expression.as_deref().unwrap_or(""), &i.timezone, 1)?;
        }
        _ => return Err(ApiError::bad_request("INVALID_TRIGGER", "不支持的触发方式")),
    }
    validate_dag(&i.definition)?;
    validate_definition_permissions(s, u, &i.definition).await?;
    validate_password_parameters(s, &i.definition).await
}

async fn validate_password_parameters(s: &AppState, definition: &Value) -> ApiResult<()> {
    for node in definition
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let job_id = parse_uuid(node.get("job_config_id"))?;
        let definitions: Value = sqlx::query_scalar("SELECT v.parameter_definitions FROM gd_job_configs j JOIN gd_job_config_versions v ON v.job_config_id=j.id AND v.version=j.current_version WHERE j.id=$1")
            .bind(job_id).fetch_one(&s.db).await?;
        let parameters = node.get("parameters").and_then(Value::as_object);
        for name in crate::crypto::password_names(&definitions) {
            let valid = parameters
                .and_then(|values| values.get(&name))
                .is_some_and(|value| {
                    value.as_str().is_some_and(|plain| !plain.is_empty())
                        || value.get("$encrypted").is_some()
                });
            if !valid {
                return Err(ApiError::bad_request(
                    "PASSWORD_REQUIRED",
                    format!("Password 参数 {name} 必须在执行前重新填写"),
                ));
            }
        }
    }
    Ok(())
}
async fn validate_definition_permissions(
    s: &AppState,
    u: &CurrentUser,
    d: &Value,
) -> ApiResult<()> {
    for node in d.get("nodes").and_then(Value::as_array).unwrap_or(&vec![]) {
        let jid = parse_uuid(node.get("job_config_id"))?;
        let cid:Option<Uuid>=sqlx::query_scalar("SELECT i.component_id FROM gd_job_configs j JOIN gd_component_instances i ON i.id=j.component_instance_id JOIN gd_environments e ON e.id=i.environment_id WHERE j.id=$1 AND j.status='ACTIVE' AND i.status='ACTIVE' AND e.is_active").bind(jid).fetch_optional(&s.db).await?;
        let cid = cid
            .ok_or_else(|| ApiError::bad_request("JOB_UNAVAILABLE", "任务包含不可用的 Job 配置"))?;
        crate::auth::component_permission(&s.db, u, cid, false).await?;
    }
    Ok(())
}
fn validate_dag(d: &Value) -> ApiResult<()> {
    let nodes = d
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| ApiError::bad_request("EMPTY_TASK", "任务至少包含一个节点"))?;
    if nodes.is_empty() {
        return Err(ApiError::bad_request("EMPTY_TASK", "任务至少包含一个节点"));
    }
    let mut deps: HashMap<String, Vec<String>> = HashMap::new();
    let mut names = HashSet::new();
    for n in nodes {
        let key = n
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError::bad_request("INVALID_NODE", "节点缺少 key"))?
            .to_owned();
        if !names.insert(key.clone()) {
            return Err(ApiError::bad_request("DUPLICATE_NODE", "节点 key 必须唯一"));
        }
        let ds = n
            .get("dependencies")
            .and_then(Value::as_array)
            .map(|v| {
                v.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        deps.insert(key, ds);
    }
    for ds in deps.values() {
        if ds.iter().any(|d| !names.contains(d)) {
            return Err(ApiError::bad_request("DANGLING_DEPENDENCY", "存在悬空依赖"));
        }
    }
    fn visit(
        k: &str,
        deps: &HashMap<String, Vec<String>>,
        visiting: &mut HashSet<String>,
        done: &mut HashSet<String>,
    ) -> bool {
        if done.contains(k) {
            return true;
        }
        if !visiting.insert(k.to_owned()) {
            return false;
        }
        for d in &deps[k] {
            if !visit(d, deps, visiting, done) {
                return false;
            }
        }
        visiting.remove(k);
        done.insert(k.to_owned());
        true
    }
    let mut done = HashSet::new();
    for k in names {
        if !visit(&k, &deps, &mut HashSet::new(), &mut done) {
            return Err(ApiError::bad_request(
                "CYCLIC_DEPENDENCY",
                "任务依赖不能形成环",
            ));
        }
    }
    Ok(())
}
fn parse_uuid(v: Option<&Value>) -> ApiResult<Uuid> {
    Uuid::parse_str(v.and_then(Value::as_str).unwrap_or(""))
        .map_err(|_| ApiError::bad_request("INVALID_JOB", "Job 配置 ID 无效"))
}
type ScheduleFields = (Option<DateTime<Utc>>, Option<DateTime<Utc>>);

fn schedule_fields(i: &TaskInput) -> ApiResult<ScheduleFields> {
    match i.trigger_type.as_str() {
        "ONCE" => Ok((i.scheduled_at, i.scheduled_at)),
        "CRON" => Ok((
            next_times(i.cron_expression.as_deref().unwrap_or(""), &i.timezone, 1)?
                .first()
                .copied(),
            None,
        )),
        _ => Ok((None, None)),
    }
}
pub fn next_times(expr: &str, tz_name: &str, count: usize) -> ApiResult<Vec<DateTime<Utc>>> {
    if expr.split_whitespace().count() != 5 {
        return Err(ApiError::bad_request(
            "INVALID_CRON",
            "Cron 必须为标准五段表达式",
        ));
    }
    let tz = Tz::from_str(tz_name)
        .map_err(|_| ApiError::bad_request("INVALID_TIMEZONE", "时区必须为有效的 IANA 时区"))?;
    let schedule = Schedule::from_str(&format!("0 {expr}"))
        .map_err(|e| ApiError::bad_request("INVALID_CRON", e.to_string()))?;
    let local = tz.from_utc_datetime(&Utc::now().naive_utc());
    Ok(schedule
        .after(&local)
        .take(count)
        .map(|x| x.with_timezone(&Utc))
        .collect())
}
fn redact(text: &str) -> String {
    text.lines()
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            if lower.contains("password=") || lower.contains("token=") {
                "[敏感日志已脱敏]"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn clear_encrypted(value: &mut Value) {
    match value {
        Value::Object(map) if map.contains_key("$encrypted") => *value = Value::Null,
        Value::Object(map) => map.values_mut().for_each(clear_encrypted),
        Value::Array(items) => items.iter_mut().for_each(clear_encrypted),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{next_times, normalize_level_dependencies, validate_dag};
    use serde_json::json;
    #[test]
    fn rejects_cycles() {
        assert!(validate_dag(&json!({"nodes":[{"key":"a","dependencies":["b"]},{"key":"b","dependencies":["a"]}]})).is_err());
    }
    #[test]
    fn every_node_waits_for_the_entire_previous_level() {
        let mut definition = json!({"nodes":[
            {"key":"build-a","dependencies":[]},
            {"key":"build-b","dependencies":[]},
            {"key":"deploy","dependencies":["build-a"]}
        ]});
        normalize_level_dependencies(&mut definition).unwrap();
        assert_eq!(
            definition["nodes"][2]["dependencies"],
            json!(["build-a", "build-b"])
        );
    }
    #[test]
    fn cron_has_five_future_values() {
        assert_eq!(
            next_times("*/5 * * * *", "Asia/Shanghai", 5).unwrap().len(),
            5
        );
    }
}

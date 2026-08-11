use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{
    app::AppState,
    error::{ApiError, ApiResult},
};

#[derive(Debug, FromRow)]
struct WorkNode {
    id: Uuid,
    status: String,
    parameters: Value,
    timeout_seconds: i32,
    queue_id: Option<i64>,
    build_number: Option<i64>,
    submitted_at: Option<DateTime<Utc>>,
    jenkins_url: String,
    request_timeout_seconds: i32,
    job_full_name: String,
    environment_id: Uuid,
}

pub async fn create_execution(
    state: &AppState,
    task_id: Uuid,
    version: i32,
    trigger_key: String,
    trigger_type: &str,
    actor: Option<Uuid>,
    scheduled_at: Option<DateTime<Utc>>,
) -> ApiResult<Uuid> {
    let definition: Value = sqlx::query_scalar(
        "SELECT definition FROM gd_task_versions WHERE task_id=$1 AND version=$2",
    )
    .bind(task_id)
    .bind(version)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError::not_found("任务版本不存在"))?;
    let status = if scheduled_at.is_some_and(|t| t > Utc::now()) {
        "SCHEDULED"
    } else {
        "RUNNING"
    };
    let mut tx = state.db.begin().await?;
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM gd_executions WHERE trigger_key=$1")
            .bind(&trigger_key)
            .fetch_optional(&mut *tx)
            .await?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let eid:Uuid=sqlx::query_scalar("INSERT INTO gd_executions(user_id,task_id,task_version,trigger_key,trigger_type,status,snapshot,scheduled_at,started_at,created_by) SELECT user_id,id,$2,$3,$4,$5,$6,$7,CASE WHEN $5='RUNNING' THEN now() END,$8 FROM gd_tasks WHERE id=$1 RETURNING id").bind(task_id).bind(version).bind(trigger_key).bind(trigger_type).bind(status).bind(&definition).bind(scheduled_at).bind(actor).fetch_one(&mut *tx).await?;
    let nodes = definition
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| ApiError::bad_request("EMPTY_TASK", "任务没有节点"))?;
    for node in nodes {
        let key = node.get("key").and_then(Value::as_str).unwrap_or("");
        let name = node.get("name").and_then(Value::as_str).unwrap_or(key);
        let jid = Uuid::parse_str(
            node.get("job_config_id")
                .and_then(Value::as_str)
                .unwrap_or(""),
        )
        .map_err(|_| ApiError::bad_request("INVALID_JOB", "Job 配置 ID 无效"))?;
        let env:Option<Uuid>=sqlx::query_scalar("SELECT i.environment_id FROM gd_job_configs j JOIN gd_component_instances i ON i.id=j.component_instance_id WHERE j.id=$1").bind(jid).fetch_optional(&mut *tx).await?;
        let env = env.ok_or_else(|| ApiError::bad_request("JOB_UNAVAILABLE", "Job 配置不存在"))?;
        let deps = node
            .get("dependencies")
            .cloned()
            .unwrap_or_else(|| json!([]));
        let params = node.get("parameters").cloned().unwrap_or_else(|| json!({}));
        let timeout = node
            .get("timeout_seconds")
            .and_then(Value::as_i64)
            .unwrap_or(3600)
            .clamp(30, 86400) as i32;
        let node_status = if status == "SCHEDULED" || deps.as_array().is_some_and(|x| !x.is_empty())
        {
            "WAITING_DEPENDENCY"
        } else {
            "PENDING"
        };
        sqlx::query("INSERT INTO gd_node_executions(execution_id,node_key,node_name,environment_id,job_config_id,status,dependencies,parameters,timeout_seconds) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(eid).bind(key).bind(name).bind(env).bind(jid).bind(node_status).bind(deps).bind(params).bind(timeout).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(eid)
}

pub async fn request_cancel(state: &AppState, id: Uuid) -> ApiResult<()> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT status FROM gd_executions WHERE id=$1 AND status IN('SCHEDULED','RUNNING','CANCELING')",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    if status.is_none() {
        return Err(ApiError::conflict("执行已结束，不能停止"));
    }
    let builds = sqlx::query_as::<_, (String, String, i32, i64)>(
        "SELECT e.jenkins_url,j.job_full_name,e.request_timeout_seconds,n.build_number FROM gd_node_executions n JOIN gd_environments e ON e.id=n.environment_id JOIN gd_job_configs j ON j.id=n.job_config_id WHERE n.execution_id=$1 AND n.status IN('RUNNING','UNKNOWN') AND n.build_number IS NOT NULL",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    for (jenkins_url, job_full_name, timeout, build_number) in builds {
        let _ = state
            .jenkins
            .stop_build(
                &jenkins_url,
                &job_full_name,
                build_number,
                timeout as u64,
                false,
            )
            .await;
    }
    sqlx::query("UPDATE gd_node_executions SET status='CANCELED',finished_at=now(),updated_at=now() WHERE execution_id=$1 AND status IN('PENDING','WAITING_DEPENDENCY','QUEUED','RUNNING','UNKNOWN')").bind(id).execute(&state.db).await?;
    sqlx::query("UPDATE gd_executions SET status='CANCELED',finished_at=now() WHERE id=$1")
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(())
}

pub fn spawn_background(state: AppState) {
    let scheduler = state.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = scheduler_tick(&scheduler).await {
                tracing::error!(error=%e,"scheduler tick failed");
            }
            tokio::time::sleep(scheduler.config.scheduler_interval).await;
        }
    });
    let worker = state.clone();
    tokio::spawn(async move {
        loop {
            if let Err(e) = worker_tick(&worker).await {
                tracing::error!(error=%e,"worker tick failed");
            }
            tokio::time::sleep(worker.config.worker_interval).await;
        }
    });
}

async fn scheduler_tick(state: &AppState) -> anyhow::Result<()> {
    let mut tx = state.db.begin().await?;
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_xact_lock(710031)")
        .fetch_one(&mut *tx)
        .await?;
    if !locked {
        return Ok(());
    }
    let due=sqlx::query_as::<_,(Uuid,i32,String,Option<DateTime<Utc>>,Option<String>,String)>("SELECT id,current_version,trigger_type,next_run_at,cron_expression,timezone FROM gd_tasks WHERE NOT is_deleted AND is_enabled AND next_run_at<=now() ORDER BY next_run_at FOR UPDATE SKIP LOCKED LIMIT 50").fetch_all(&mut *tx).await?;
    tx.commit().await?;
    for (task, version, trigger, next, cron, tz) in due {
        let scheduled = next.unwrap_or_else(Utc::now);
        let key = format!("schedule:{task}:{}", scheduled.timestamp());
        let overlap:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM gd_executions WHERE task_id=$1 AND status IN('SCHEDULED','RUNNING','CANCELING'))").bind(task).fetch_one(&state.db).await?;
        if trigger == "CRON" && overlap {
            sqlx::query("INSERT INTO gd_schedule_events(task_id,event_type,scheduled_at,reason) VALUES($1,'SKIPPED_OVERLAP',$2,'上一次周期执行尚未结束')").bind(task).bind(scheduled).execute(&state.db).await?;
        } else {
            let _ = create_execution(state, task, version, key, &trigger, None, None).await?;
        }
        if trigger == "CRON" {
            let expr = cron.unwrap_or_default();
            let next = crate::routes::tasks::next_times(&expr, &tz, 1)?
                .first()
                .copied();
            sqlx::query("UPDATE gd_tasks SET next_run_at=$2,updated_at=now() WHERE id=$1")
                .bind(task)
                .bind(next)
                .execute(&state.db)
                .await?;
        } else {
            sqlx::query(
                "UPDATE gd_tasks SET is_enabled=false,next_run_at=NULL,updated_at=now() WHERE id=$1",
            )
            .bind(task)
            .execute(&state.db)
            .await?;
        }
    }
    Ok(())
}

async fn worker_tick(state: &AppState) -> anyhow::Result<()> {
    let Some(worker_user_id) = *state.worker_user_id.read().await else {
        return Ok(());
    };
    sqlx::query("UPDATE gd_executions SET status='RUNNING',started_at=COALESCE(started_at,now()) WHERE status='SCHEDULED' AND scheduled_at<=now() AND user_id=$1").bind(worker_user_id).execute(&state.db).await?;
    release_dependencies(&state.db, worker_user_id).await?;
    let running: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM gd_node_executions WHERE status IN('QUEUED','RUNNING','UNKNOWN')",
    )
    .fetch_one(&state.db)
    .await?;
    let capacity = state
        .config
        .global_concurrency
        .saturating_sub(running as usize);
    let mut nodes=sqlx::query_as::<_,WorkNode>("SELECT n.id,n.status,n.parameters,n.timeout_seconds,n.queue_id,n.build_number,n.submitted_at,e.jenkins_url,e.request_timeout_seconds,j.job_full_name,n.environment_id FROM gd_node_executions n JOIN gd_executions x ON x.id=n.execution_id JOIN gd_environments e ON e.id=n.environment_id JOIN gd_job_configs j ON j.id=n.job_config_id WHERE x.status='RUNNING' AND x.user_id=$1 AND n.status IN('PENDING','QUEUED','RUNNING','UNKNOWN') AND (n.claim_expires_at IS NULL OR n.claim_expires_at<now()) ORDER BY CASE n.status WHEN 'RUNNING' THEN 0 WHEN 'UNKNOWN' THEN 1 WHEN 'QUEUED' THEN 2 ELSE 3 END,n.updated_at LIMIT 100").bind(worker_user_id).fetch_all(&state.db).await?;
    let mut submitted = 0;
    for node in nodes.drain(..) {
        if node.status == "PENDING" && submitted >= capacity {
            continue;
        }
        if claim(&state.db, node.id, state.instance_id).await? {
            if node.status == "PENDING" {
                let per:i64=sqlx::query_scalar("SELECT count(*) FROM gd_node_executions WHERE environment_id=$1 AND status IN('QUEUED','RUNNING','UNKNOWN')").bind(node.environment_id).fetch_one(&state.db).await?;
                if per as usize >= state.config.per_jenkins_concurrency {
                    unclaim(&state.db, node.id).await?;
                    continue;
                }
                submitted += 1;
            }
            if let Err(e) = process_node(state, &node).await {
                mark_unknown_or_failed(&state.db, &node, e.to_string()).await?;
            }
            unclaim(&state.db, node.id).await?;
        }
    }
    finalize_executions(&state.db, worker_user_id).await?;
    Ok(())
}

async fn claim(db: &PgPool, id: Uuid, owner: Uuid) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query("UPDATE gd_node_executions SET claimed_by=$2,claim_expires_at=now()+interval '30 seconds' WHERE id=$1 AND (claim_expires_at IS NULL OR claim_expires_at<now())").bind(id).bind(owner).execute(db).await?.rows_affected()==1)
}
async fn unclaim(db: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE gd_node_executions SET claimed_by=NULL,claim_expires_at=NULL WHERE id=$1")
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

async fn process_node(state: &AppState, n: &WorkNode) -> anyhow::Result<()> {
    if n.submitted_at
        .is_some_and(|at| Utc::now() - at > Duration::seconds(n.timeout_seconds as i64))
    {
        sqlx::query("UPDATE gd_node_executions SET status='TIMED_OUT',finished_at=now(),error_summary='节点执行超时',updated_at=now() WHERE id=$1").bind(n.id).execute(&state.db).await?;
        return Ok(());
    }
    match n.status.as_str() {
        "PENDING" => {
            let usable:bool=sqlx::query_scalar("SELECT e.is_active AND i.status='ACTIVE' AND j.status='ACTIVE' FROM gd_job_configs j JOIN gd_component_instances i ON i.id=j.component_instance_id JOIN gd_environments e ON e.id=i.environment_id WHERE j.id=(SELECT job_config_id FROM gd_node_executions WHERE id=$1)").bind(n.id).fetch_one(&state.db).await?;
            if !usable {
                sqlx::query("UPDATE gd_node_executions SET status='FAILED',error_summary='环境、实例或 Job 配置不可用',finished_at=now(),updated_at=now() WHERE id=$1").bind(n.id).execute(&state.db).await?;
                return Ok(());
            }
            let (job_config_id, saved_definitions): (Uuid, Value) = sqlx::query_as(
                "SELECT j.id,v.parameter_definitions FROM gd_node_executions n JOIN gd_job_configs j ON j.id=n.job_config_id JOIN gd_job_config_versions v ON v.job_config_id=j.id AND v.version=j.current_version WHERE n.id=$1",
            )
            .bind(n.id)
            .fetch_one(&state.db)
            .await?;
            let raw_definition = state
                .jenkins
                .job_definition(
                    &n.jenkins_url,
                    &n.job_full_name,
                    n.request_timeout_seconds as u64,
                    false,
                )
                .await?;
            let latest = crate::routes::catalog::extract_parameter_definitions(&raw_definition);
            let comparison =
                crate::routes::catalog::compare_definitions(&saved_definitions, &latest);
            if !comparison
                .get("compatible")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            {
                sqlx::query(
                    "UPDATE gd_job_configs SET status='STALE',updated_at=now() WHERE id=$1",
                )
                .bind(job_config_id)
                .execute(&state.db)
                .await?;
                sqlx::query("UPDATE gd_node_executions SET status='FAILED',error_summary='Jenkins 参数定义发生不兼容变更，请更新 Job 配置',finished_at=now(),updated_at=now() WHERE id=$1")
                    .bind(n.id)
                    .execute(&state.db)
                    .await?;
                return Ok(());
            }
            let parameters = state.crypto.decrypt_parameters(&n.parameters)?;
            let location = state
                .jenkins
                .trigger(
                    &n.jenkins_url,
                    &n.job_full_name,
                    &parameters,
                    n.request_timeout_seconds as u64,
                    false,
                )
                .await?;
            let qid = location
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .and_then(|v| v.parse::<i64>().ok())
                .ok_or_else(|| anyhow::anyhow!("无法从 Location 解析 queue id"))?;
            sqlx::query("UPDATE gd_node_executions SET status='QUEUED',queue_id=$2,queue_url=$3,submitted_at=now(),updated_at=now() WHERE id=$1").bind(n.id).bind(qid).bind(location).execute(&state.db).await?;
        }
        "QUEUED" | "UNKNOWN" if n.build_number.is_none() => {
            let q = state
                .jenkins
                .queue(
                    &n.jenkins_url,
                    n.queue_id.ok_or_else(|| anyhow::anyhow!("queue id 丢失"))?,
                    n.request_timeout_seconds as u64,
                    false,
                )
                .await?;
            if q.cancelled == Some(true) {
                sqlx::query("UPDATE gd_node_executions SET status='CANCELED',finished_at=now(),updated_at=now() WHERE id=$1").bind(n.id).execute(&state.db).await?;
            } else if let Some(x) = q.executable {
                sqlx::query("UPDATE gd_node_executions SET status='RUNNING',build_number=$2,build_url=$3,blocking_reason=NULL,started_at=now(),updated_at=now() WHERE id=$1").bind(n.id).bind(x.number).bind(x.url).execute(&state.db).await?;
            } else {
                sqlx::query("UPDATE gd_node_executions SET status='QUEUED',blocking_reason=$2,updated_at=now() WHERE id=$1").bind(n.id).bind(q.why).execute(&state.db).await?;
            }
        }
        "RUNNING" | "UNKNOWN" => {
            let b = match state
                .jenkins
                .build(
                    &n.jenkins_url,
                    &n.job_full_name,
                    n.build_number
                        .ok_or_else(|| anyhow::anyhow!("build number 丢失"))?,
                    n.request_timeout_seconds as u64,
                    false,
                )
                .await
            {
                Ok(build) => build,
                Err(error) => {
                    tracing::warn!(node_id=%n.id, error=%error, "Jenkins build status temporarily unavailable");
                    sqlx::query("UPDATE gd_node_executions SET status='RUNNING',error_summary=$2,updated_at=now() WHERE id=$1")
                        .bind(n.id)
                        .bind(format!("Jenkins 状态查询暂时不可用：{}", sanitize(&error.to_string())))
                        .execute(&state.db)
                        .await?;
                    return Ok(());
                }
            };
            if b.building {
                sqlx::query(
                    "UPDATE gd_node_executions SET status='RUNNING',error_summary=NULL,updated_at=now() WHERE id=$1",
                )
                .bind(n.id)
                .execute(&state.db)
                .await?;
            } else if let Some(result) = b.result {
                let success = result == "SUCCESS";
                sqlx::query("UPDATE gd_node_executions SET status=$2,finished_at=now(),error_summary=CASE WHEN $2='FAILED' THEN $3 ELSE NULL END,updated_at=now() WHERE id=$1").bind(n.id).bind(if success{"SUCCESS"}else{"FAILED"}).bind(format!("Jenkins 结果：{result}")).execute(&state.db).await?;
            } else {
                sqlx::query("UPDATE gd_node_executions SET status='RUNNING',error_summary='Jenkins 尚未返回明确执行结果',updated_at=now() WHERE id=$1")
                    .bind(n.id)
                    .execute(&state.db)
                    .await?;
            }
        }
        _ => {}
    }
    Ok(())
}

async fn mark_unknown_or_failed(db: &PgPool, n: &WorkNode, error: String) -> anyhow::Result<()> {
    let status = if n.submitted_at.is_some() || n.status != "PENDING" {
        n.status.as_str()
    } else {
        "FAILED"
    };
    sqlx::query("UPDATE gd_node_executions SET status=$2,error_summary=$3,updated_at=now(),finished_at=CASE WHEN $2='FAILED' THEN now() ELSE finished_at END WHERE id=$1").bind(n.id).bind(status).bind(sanitize(&error)).execute(db).await?;
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum DependencyDecision {
    Wait,
    Ready,
    Skip,
}

fn dependency_decision(keys: &[String], statuses: &HashMap<String, String>) -> DependencyDecision {
    if keys.iter().any(|key| {
        statuses.get(key).is_some_and(|status| {
            matches!(
                status.as_str(),
                "FAILED" | "CANCELED" | "TIMED_OUT" | "SKIPPED"
            )
        })
    }) {
        DependencyDecision::Skip
    } else if keys
        .iter()
        .all(|key| statuses.get(key).is_some_and(|status| status == "SUCCESS"))
    {
        DependencyDecision::Ready
    } else {
        DependencyDecision::Wait
    }
}

async fn release_dependencies(db: &PgPool, user_id: Uuid) -> anyhow::Result<()> {
    let rows=sqlx::query_as::<_,(Uuid,Uuid,String,Value)>("SELECT id,execution_id,node_key,dependencies FROM gd_node_executions WHERE status='WAITING_DEPENDENCY' AND execution_id IN(SELECT id FROM gd_executions WHERE status='RUNNING' AND user_id=$1)").bind(user_id).fetch_all(db).await?;
    for (id, eid, _key, deps) in rows {
        let keys = deps
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|x| x.as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        if keys.is_empty() {
            sqlx::query(
                "UPDATE gd_node_executions SET status='PENDING',updated_at=now() WHERE id=$1",
            )
            .bind(id)
            .execute(db)
            .await?;
            continue;
        }
        let statuses = sqlx::query_as::<_, (String, String)>(
            "SELECT node_key,status FROM gd_node_executions WHERE execution_id=$1",
        )
        .bind(eid)
        .fetch_all(db)
        .await?
        .into_iter()
        .collect::<HashMap<_, _>>();
        match dependency_decision(&keys, &statuses) {
            DependencyDecision::Skip => {
                sqlx::query("UPDATE gd_node_executions SET status='SKIPPED',error_summary='前置节点未成功',finished_at=now(),updated_at=now() WHERE id=$1").bind(id).execute(db).await?;
            }
            DependencyDecision::Ready => {
                sqlx::query(
                    "UPDATE gd_node_executions SET status='PENDING',updated_at=now() WHERE id=$1",
                )
                .bind(id)
                .execute(db)
                .await?;
            }
            DependencyDecision::Wait => {}
        }
    }
    Ok(())
}

async fn finalize_executions(db: &PgPool, user_id: Uuid) -> anyhow::Result<()> {
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM gd_executions WHERE status IN('RUNNING','CANCELING') AND user_id=$1",
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    for id in ids {
        let states: Vec<String> =
            sqlx::query_scalar("SELECT status FROM gd_node_executions WHERE execution_id=$1")
                .bind(id)
                .fetch_all(db)
                .await?;
        if states.iter().any(|s| {
            matches!(
                s.as_str(),
                "PENDING" | "WAITING_DEPENDENCY" | "QUEUED" | "RUNNING" | "UNKNOWN"
            )
        }) {
            continue;
        }
        let current: String = sqlx::query_scalar("SELECT status FROM gd_executions WHERE id=$1")
            .bind(id)
            .fetch_one(db)
            .await?;
        let final_status = if current == "CANCELING" {
            "CANCELED"
        } else if states.iter().all(|s| s == "SUCCESS") {
            "SUCCESS"
        } else {
            "FAILED"
        };
        sqlx::query("UPDATE gd_executions SET status=$2,finished_at=now() WHERE id=$1")
            .bind(id)
            .bind(final_status)
            .execute(db)
            .await?;
    }
    Ok(())
}

fn sanitize(v: &str) -> String {
    v.replace(['\r', '\n'], " ").chars().take(500).collect()
}

#[cfg(test)]
mod tests {
    use super::{DependencyDecision, dependency_decision};
    use std::collections::HashMap;

    #[test]
    fn serial_node_waits_until_every_parallel_predecessor_succeeds() {
        let dependencies = vec!["build-a".to_owned(), "build-b".to_owned()];
        let statuses = HashMap::from([
            ("build-a".to_owned(), "SUCCESS".to_owned()),
            ("build-b".to_owned(), "RUNNING".to_owned()),
        ]);
        assert_eq!(
            dependency_decision(&dependencies, &statuses),
            DependencyDecision::Wait
        );
    }

    #[test]
    fn serial_node_is_released_after_every_predecessor_succeeds() {
        let dependencies = vec!["build-a".to_owned(), "build-b".to_owned()];
        let statuses = HashMap::from([
            ("build-a".to_owned(), "SUCCESS".to_owned()),
            ("build-b".to_owned(), "SUCCESS".to_owned()),
        ]);
        assert_eq!(
            dependency_decision(&dependencies, &statuses),
            DependencyDecision::Ready
        );
    }

    #[test]
    fn failed_predecessor_skips_serial_downstream() {
        let dependencies = vec!["build-a".to_owned(), "build-b".to_owned()];
        let statuses = HashMap::from([
            ("build-a".to_owned(), "SUCCESS".to_owned()),
            ("build-b".to_owned(), "FAILED".to_owned()),
        ]);
        assert_eq!(
            dependency_decision(&dependencies, &statuses),
            DependencyDecision::Skip
        );
    }

    #[test]
    fn temporarily_unknown_predecessor_keeps_downstream_waiting() {
        let dependencies = vec!["build-a".to_owned()];
        let statuses = HashMap::from([("build-a".to_owned(), "UNKNOWN".to_owned())]);
        assert_eq!(
            dependency_decision(&dependencies, &statuses),
            DependencyDecision::Wait
        );
    }
}

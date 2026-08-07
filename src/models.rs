use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct PageQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size")]
    pub page_size: i64,
    #[serde(default)]
    pub q: String,
}

#[derive(Debug, Deserialize)]
pub struct EnvironmentQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size")]
    pub page_size: i64,
    #[serde(default)]
    pub q: String,
    pub customer_id: Option<Uuid>,
    #[serde(default)]
    pub deployment_domain: String,
}

#[derive(Debug, Deserialize)]
pub struct InstanceQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_page_size")]
    pub page_size: i64,
    #[serde(default)]
    pub q: String,
    pub component_id: Option<Uuid>,
    pub customer_id: Option<Uuid>,
    pub environment_id: Option<Uuid>,
    #[serde(default)]
    pub deployment_domain: String,
}
impl InstanceQuery {
    pub fn limit(&self) -> i64 {
        self.page_size.clamp(1, 100)
    }
    pub fn offset(&self) -> i64 {
        (self.page.max(1) - 1) * self.limit()
    }
    pub fn pattern(&self) -> String {
        format!("%{}%", self.q.trim())
    }
}
impl EnvironmentQuery {
    pub fn limit(&self) -> i64 {
        self.page_size.clamp(1, 100)
    }
    pub fn offset(&self) -> i64 {
        (self.page.max(1) - 1) * self.limit()
    }
    pub fn pattern(&self) -> String {
        format!("%{}%", self.q.trim())
    }
}
fn default_page() -> i64 {
    1
}
fn default_page_size() -> i64 {
    20
}
impl PageQuery {
    pub fn limit(&self) -> i64 {
        self.page_size.clamp(1, 100)
    }
    pub fn offset(&self) -> i64 {
        (self.page.max(1) - 1) * self.limit()
    }
    pub fn pattern(&self) -> String {
        format!("%{}%", self.q.trim())
    }
}

#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub page: i64,
    pub page_size: i64,
    pub total: i64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct UserView {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub is_admin: bool,
    pub is_active: bool,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct CustomerView {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub is_active: bool,
    pub version: i32,
    pub environment_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct ComponentView {
    pub id: Uuid,
    pub code: String,
    pub name: String,
    pub is_public: bool,
    pub is_active: bool,
    pub version: i32,
    pub owner_names: String,
    pub developer_names: String,
    pub tester_names: String,
    pub owner_id: Option<Uuid>,
    pub developer_ids: Vec<Uuid>,
    pub tester_ids: Vec<Uuid>,
    pub instance_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct EnvironmentView {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub customer_code: String,
    pub customer_name: String,
    pub deployment_domain: String,
    pub code: String,
    pub name: String,
    pub jenkins_url: String,
    pub request_timeout_seconds: i32,
    pub notes: String,
    pub is_active: bool,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct InstanceView {
    pub id: Uuid,
    pub name: String,
    pub component_id: Uuid,
    pub component_name: String,
    pub environment_id: Uuid,
    pub environment_name: String,
    pub customer_id: Uuid,
    pub customer_name: String,
    pub deployment_domain: String,
    pub folder_full_name: String,
    pub folder_url: Option<String>,
    pub status: String,
    pub notes: String,
    pub custom_fields: serde_json::Value,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct JobConfigView {
    pub id: Uuid,
    pub component_instance_id: Uuid,
    pub instance_name: String,
    pub component_id: Uuid,
    pub component_name: String,
    pub environment_id: Uuid,
    pub environment_name: String,
    pub customer_id: Uuid,
    pub customer_name: String,
    pub deployment_domain: String,
    pub display_name: String,
    pub description: String,
    pub job_full_name: String,
    pub job_url: Option<String>,
    pub status: String,
    pub current_version: i32,
    pub version: i32,
    pub parameter_definitions: serde_json::Value,
    pub parameter_presets: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct TaskView {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub creator_id: Uuid,
    pub creator_name: String,
    pub trigger_type: String,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub cron_expression: Option<String>,
    pub timezone: String,
    pub is_enabled: bool,
    pub current_version: i32,
    pub version: i32,
    pub next_run_at: Option<DateTime<Utc>>,
    pub definition: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct ExecutionView {
    pub id: Uuid,
    pub task_id: Uuid,
    pub task_name: String,
    pub task_version: i32,
    pub trigger_type: String,
    pub status: String,
    pub snapshot: serde_json::Value,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct NodeExecutionView {
    pub id: Uuid,
    pub execution_id: Uuid,
    pub node_key: String,
    pub node_name: String,
    pub dependencies: serde_json::Value,
    pub status: String,
    pub queue_id: Option<i64>,
    pub queue_url: Option<String>,
    pub build_number: Option<i64>,
    pub build_url: Option<String>,
    pub blocking_reason: Option<String>,
    pub error_summary: Option<String>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

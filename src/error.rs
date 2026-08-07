use axum::{Json, http::StatusCode, response::IntoResponse};
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("{1}")]
    Client(StatusCode, String, &'static str, Option<Value>),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fields: Option<Value>,
}

impl ApiError {
    pub fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self::Client(StatusCode::BAD_REQUEST, message.into(), code, None)
    }
    pub fn unauthorized() -> Self {
        Self::Client(
            StatusCode::UNAUTHORIZED,
            "请先登录".into(),
            "UNAUTHORIZED",
            None,
        )
    }
    pub fn forbidden() -> Self {
        Self::Client(
            StatusCode::FORBIDDEN,
            "没有执行此操作的权限".into(),
            "FORBIDDEN",
            None,
        )
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::Client(StatusCode::NOT_FOUND, message.into(), "NOT_FOUND", None)
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Client(StatusCode::CONFLICT, message.into(), "CONFLICT", None)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, body) = match self {
            Self::Client(status, message, code, fields) => (
                status,
                ErrorBody {
                    code,
                    message,
                    fields,
                },
            ),
            Self::Database(error) => {
                tracing::error!(error = %error, "database request failed");
                if error
                    .as_database_error()
                    .is_some_and(|e| e.is_unique_violation())
                {
                    (
                        StatusCode::CONFLICT,
                        ErrorBody {
                            code: "DUPLICATE",
                            message: "数据已存在或违反唯一性约束".into(),
                            fields: None,
                        },
                    )
                } else {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        ErrorBody {
                            code: "DATABASE_ERROR",
                            message: "数据库操作失败".into(),
                            fields: None,
                        },
                    )
                }
            }
            Self::Internal(error) => {
                tracing::error!(error = %error, "request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ErrorBody {
                        code: "INTERNAL_ERROR",
                        message: "服务内部错误".into(),
                        fields: None,
                    },
                )
            }
        };
        (status, Json(body)).into_response()
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

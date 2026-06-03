use crate::app_state::AppState;
use crate::auth::BusinessAuth;
use crate::error::{AppError, AppResult};
use axum::{extract::State, Extension, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct RegisterWebhookRequest {
    pub url: String,
    #[serde(default)]
    pub secret: Option<String>,
}

#[derive(Serialize)]
pub struct WebhookEndpointResponse {
    pub id: Uuid,
    pub url: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn register_webhook(
    Extension(auth): Extension<BusinessAuth>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<RegisterWebhookRequest>,
) -> AppResult<Json<WebhookEndpointResponse>> {
    if body.url.trim().is_empty() {
        return Err(AppError::BadRequest("url is required".into()));
    }
    let secret = body.secret.unwrap_or_else(|| {
        uuid::Uuid::new_v4().to_string().replace('-', "")
    });

    let row = sqlx::query_as::<_, EndpointRow>(
        r#"
        INSERT INTO webhook_endpoints (business_id, url, secret)
        VALUES ($1, $2, $3)
        RETURNING id, url, created_at
        "#,
    )
    .bind(auth.business_id)
    .bind(body.url.trim())
    .bind(&secret)
    .fetch_one(state.pool.as_ref())
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(WebhookEndpointResponse {
        id: row.id,
        url: row.url,
        created_at: row.created_at,
    }))
}

#[derive(sqlx::FromRow)]
struct EndpointRow {
    id: Uuid,
    url: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

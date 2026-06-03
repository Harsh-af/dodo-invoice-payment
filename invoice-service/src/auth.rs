use crate::error::{AppError, AppResult};
use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};
use sqlx::PgPool;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct BusinessAuth {
    pub business_id: Uuid,
    pub api_key_id: Uuid,
}

use crate::app_state::AppState;

pub async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let auth = extract_auth(state.pool.as_ref(), req.headers()).await?;
    req.extensions_mut().insert(auth);
    Ok(next.run(req).await)
}

pub async fn extract_auth(pool: &PgPool, headers: &axum::http::HeaderMap) -> AppResult<BusinessAuth> {
    let raw = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::Unauthorized("missing Authorization header".into()))?;

    let key = raw
        .strip_prefix("Bearer ")
        .or_else(|| raw.strip_prefix("bearer "))
        .ok_or_else(|| {
            AppError::Unauthorized("Authorization must be Bearer <api_key>".into())
        })?;

    lookup_api_key(pool, key).await
}

async fn lookup_api_key(pool: &PgPool, full_key: &str) -> AppResult<BusinessAuth> {
    if full_key.len() < 12 {
        return Err(AppError::Unauthorized("invalid API key".into()));
    }
    let prefix = &full_key[..12];

    let row = sqlx::query_as::<_, ApiKeyRow>(
        r#"
        SELECT id, business_id, key_hash
        FROM api_keys
        WHERE key_prefix = $1 AND revoked_at IS NULL
        "#,
    )
    .bind(prefix)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let row = row.ok_or_else(|| AppError::Unauthorized("invalid API key".into()))?;

    let valid = bcrypt::verify(full_key, &row.key_hash).unwrap_or(false);
    if !valid {
        return Err(AppError::Unauthorized("invalid API key".into()));
    }

    Ok(BusinessAuth {
        business_id: row.business_id,
        api_key_id: row.id,
    })
}

#[derive(sqlx::FromRow)]
struct ApiKeyRow {
    id: Uuid,
    business_id: Uuid,
    key_hash: String,
}

pub fn generate_api_key() -> (String, String, String) {
    let secret: String = (0..32)
        .map(|_| {
            let idx = rand_simple() % 62;
            b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"[idx] as char
        })
        .collect();
    let full = format!("dodo_sk_{secret}");
    let prefix = full[..12].to_string();
    let hash = bcrypt::hash(&full, 12).expect("bcrypt hash");
    (full, prefix, hash)
}

fn rand_simple() -> usize {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::{SystemTime, UNIX_EPOCH};
    let mut h = DefaultHasher::new();
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .hash(&mut h);
    (h.finish() as usize)
}

use crate::app_state::AppState;
use crate::auth::BusinessAuth;
use crate::error::{AppError, AppResult};
use axum::{extract::State, Extension, Json};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CreateCustomerRequest {
    pub name: String,
    pub email: String,
}

#[derive(Serialize)]
pub struct CustomerResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
}

pub async fn create_customer(
    Extension(auth): Extension<BusinessAuth>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateCustomerRequest>,
) -> AppResult<Json<CustomerResponse>> {
    if body.name.trim().is_empty() || body.email.trim().is_empty() {
        return Err(AppError::BadRequest("name and email are required".into()));
    }

    let row = sqlx::query_as::<_, CustomerRow>(
        r#"
        INSERT INTO customers (business_id, name, email)
        VALUES ($1, $2, $3)
        RETURNING id, name, email, created_at
        "#,
    )
    .bind(auth.business_id)
    .bind(body.name.trim())
    .bind(body.email.trim())
    .fetch_one(state.pool.as_ref())
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db) = &e {
            if db.constraint() == Some("customers_business_id_email_key") {
                return AppError::Conflict("customer with this email already exists".into());
            }
        }
        AppError::Internal(e.into())
    })?;

    Ok(Json(row.into()))
}

pub async fn get_customer(
    Extension(auth): Extension<BusinessAuth>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> AppResult<Json<CustomerResponse>> {
    let row = sqlx::query_as::<_, CustomerRow>(
        r#"
        SELECT id, name, email, created_at
        FROM customers
        WHERE id = $1 AND business_id = $2
        "#,
    )
    .bind(id)
    .bind(auth.business_id)
    .fetch_optional(state.pool.as_ref())
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or_else(|| AppError::NotFound("customer not found".into()))?;

    Ok(Json(row.into()))
}

pub async fn list_customers(
    Extension(auth): Extension<BusinessAuth>,
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<CustomerResponse>>> {
    let rows = sqlx::query_as::<_, CustomerRow>(
        r#"
        SELECT id, name, email, created_at
        FROM customers
        WHERE business_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(auth.business_id)
    .fetch_all(state.pool.as_ref())
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[derive(sqlx::FromRow)]
struct CustomerRow {
    id: Uuid,
    name: String,
    email: String,
    created_at: DateTime<Utc>,
}

impl From<CustomerRow> for CustomerResponse {
    fn from(r: CustomerRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            email: r.email,
            created_at: r.created_at,
        }
    }
}

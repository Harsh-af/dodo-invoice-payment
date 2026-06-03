use crate::auth::BusinessAuth;
use crate::error::{AppError, AppResult};
use crate::psp::{card_token_hint, PspClient, PspOutcome};
use crate::state_machine::InvoiceState;
use crate::webhooks;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct LineItemInput {
    pub description: String,
    pub quantity: i32,
    pub unit_amount_cents: i64,
}

#[derive(Deserialize)]
pub struct CreateInvoiceRequest {
    pub customer_id: Uuid,
    pub due_date: NaiveDate,
    pub line_items: Vec<LineItemInput>,
    #[serde(default)]
    pub finalize: bool,
}

#[derive(Serialize, Deserialize)]
pub struct LineItemResponse {
    pub description: String,
    pub quantity: i32,
    pub unit_amount_cents: i64,
    pub line_total_cents: i64,
}

#[derive(Serialize, Deserialize)]
pub struct InvoiceResponse {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub state: String,
    pub total_cents: i64,
    pub due_date: NaiveDate,
    pub line_items: Vec<LineItemResponse>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct ListInvoicesQuery {
    pub state: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct PayInvoiceRequest {
    pub card_token: String,
}

#[derive(Serialize, Deserialize)]
pub struct PaymentAttemptResponse {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub state: String,
    pub psp_ref: Option<String>,
    pub failure_code: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct PayInvoiceResponse {
    pub payment_attempt: PaymentAttemptResponse,
    pub invoice: InvoiceResponse,
}

use crate::app_state::AppState;

fn compute_total(line_items: &[LineItemInput]) -> AppResult<i64> {
    let mut total: i64 = 0;
    for item in line_items {
        if item.quantity <= 0 {
            return Err(AppError::BadRequest(
                "line item quantity must be positive".into(),
            ));
        }
        if item.unit_amount_cents < 0 {
            return Err(AppError::BadRequest(
                "unit_amount_cents must be non-negative".into(),
            ));
        }
        let line = (item.quantity as i64)
            .checked_mul(item.unit_amount_cents)
            .ok_or_else(|| AppError::BadRequest("line total overflow".into()))?;
        total = total
            .checked_add(line)
            .ok_or_else(|| AppError::BadRequest("invoice total overflow".into()))?;
    }
    Ok(total)
}

pub async fn create_invoice(
    Extension(auth): Extension<BusinessAuth>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateInvoiceRequest>,
) -> AppResult<(StatusCode, Json<InvoiceResponse>)> {
    if body.line_items.is_empty() {
        return Err(AppError::BadRequest("at least one line item required".into()));
    }
    let total = compute_total(&body.line_items)?;
    let initial_state = if body.finalize {
        InvoiceState::Open
    } else {
        InvoiceState::Draft
    };

    let customer_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM customers WHERE id = $1 AND business_id = $2)",
    )
    .bind(body.customer_id)
    .bind(auth.business_id)
    .fetch_one(state.pool.as_ref())
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if !customer_exists {
        return Err(AppError::NotFound("customer not found".into()));
    }

    let mut tx = state
        .pool
        .begin()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let invoice_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO invoices (business_id, customer_id, state, total_cents, due_date)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        "#,
    )
    .bind(auth.business_id)
    .bind(body.customer_id)
    .bind(initial_state)
    .bind(total)
    .bind(body.due_date)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    for item in &body.line_items {
        sqlx::query(
            r#"
            INSERT INTO invoice_line_items (invoice_id, description, quantity, unit_amount_cents)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(invoice_id)
        .bind(&item.description)
        .bind(item.quantity)
        .bind(item.unit_amount_cents)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    }

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let invoice = load_invoice(state.pool.as_ref(), auth.business_id, invoice_id).await?;

    let payload = serde_json::json!({
        "invoice_id": invoice.id,
        "customer_id": invoice.customer_id,
        "state": invoice.state,
        "total_cents": invoice.total_cents,
    });
    webhooks::enqueue_event(
        state.pool.as_ref(),
        auth.business_id,
        "invoice.created",
        payload,
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok((StatusCode::CREATED, Json(invoice)))
}

pub async fn get_invoice(
    Extension(auth): Extension<BusinessAuth>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> AppResult<Json<InvoiceResponse>> {
    Ok(Json(
        load_invoice(state.pool.as_ref(), auth.business_id, id).await?,
    ))
}

pub async fn list_invoices(
    Extension(auth): Extension<BusinessAuth>,
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListInvoicesQuery>,
) -> AppResult<Json<Vec<InvoiceResponse>>> {
    let rows: Vec<Uuid> = if let Some(st) = &q.state {
        sqlx::query_scalar(
            r#"
            SELECT id FROM invoices
            WHERE business_id = $1 AND state::text = $2
            ORDER BY created_at DESC
            "#,
        )
        .bind(auth.business_id)
        .bind(st)
        .fetch_all(state.pool.as_ref())
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    } else {
        sqlx::query_scalar(
            r#"
            SELECT id FROM invoices
            WHERE business_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(auth.business_id)
        .fetch_all(state.pool.as_ref())
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    };

    let mut out = Vec::new();
    for id in rows {
        out.push(load_invoice(state.pool.as_ref(), auth.business_id, id).await?);
    }
    Ok(Json(out))
}

pub async fn finalize_invoice(
    Extension(auth): Extension<BusinessAuth>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> AppResult<Json<InvoiceResponse>> {
    let mut tx = state.pool.begin().await.map_err(|e| AppError::Internal(e.into()))?;
    let current = lock_invoice(&mut tx, auth.business_id, id).await?;
    current
        .state
        .transition_to(InvoiceState::Open)?;
    update_invoice_state(&mut tx, id, InvoiceState::Open).await?;
    tx.commit().await.map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(
        load_invoice(state.pool.as_ref(), auth.business_id, id).await?,
    ))
}

pub async fn void_invoice(
    Extension(auth): Extension<BusinessAuth>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> AppResult<Json<InvoiceResponse>> {
    let mut tx = state.pool.begin().await.map_err(|e| AppError::Internal(e.into()))?;
    let current = lock_invoice(&mut tx, auth.business_id, id).await?;
    if !current.state.can_void() {
        return Err(AppError::InvalidState(format!(
            "cannot void invoice in state {}",
            current.state.as_str()
        )));
    }
    current
        .state
        .transition_to(InvoiceState::Void)?;
    update_invoice_state(&mut tx, id, InvoiceState::Void).await?;
    tx.commit().await.map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(
        load_invoice(state.pool.as_ref(), auth.business_id, id).await?,
    ))
}

pub async fn pay_invoice(
    Extension(auth): Extension<BusinessAuth>,
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    Json(body): Json<PayInvoiceRequest>,
) -> AppResult<(StatusCode, Json<PayInvoiceResponse>)> {
    let idem_key = headers
        .get("Idempotency-Key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("Idempotency-Key header is required".into()))?;

    let request_hash = hash_request(&body);
    let path = format!("/invoices/{id}/pay");

    if let Some(cached) =
        check_idempotency(state.pool.as_ref(), auth.business_id, &idem_key, &request_hash).await?
    {
        return Ok(cached);
    }

    if let Some(existing_attempt) = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM payment_attempts WHERE invoice_id = $1 AND idempotency_key = $2",
    )
    .bind(id)
    .bind(&idem_key)
    .fetch_optional(state.pool.as_ref())
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    {
        let response = build_pay_response(
            state.pool.as_ref(),
            auth.business_id,
            id,
            existing_attempt,
            StatusCode::ACCEPTED,
        )
        .await?;
        return Ok((StatusCode::ACCEPTED, Json(response.1)));
    }

    let mut tx = state.pool.begin().await.map_err(|e| AppError::Internal(e.into()))?;
    let invoice = lock_invoice(&mut tx, auth.business_id, id).await?;

    if invoice.state == InvoiceState::Paid {
        return Err(AppError::Conflict("invoice is already paid".into()));
    }
    if !invoice.state.can_pay() {
        return Err(AppError::InvalidState(format!(
            "cannot pay invoice in state {}",
            invoice.state.as_str()
        )));
    }

    let pending: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id FROM payment_attempts
        WHERE invoice_id = $1 AND state = 'pending'
        LIMIT 1
        "#,
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if pending.is_some() {
        return Err(AppError::Conflict(
            "payment already in progress for this invoice".into(),
        ));
    }

    let attempt_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO payment_attempts (invoice_id, state, card_token_hint, idempotency_key)
        VALUES ($1, 'pending', $2, $3)
        RETURNING id
        "#,
    )
    .bind(id)
    .bind(card_token_hint(&body.card_token))
    .bind(&idem_key)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(db) = &e {
            if db.constraint() == Some("payment_attempts_invoice_id_idempotency_key_key") {
                return AppError::Conflict("duplicate idempotency key for this invoice".into());
            }
        }
        AppError::Internal(e.into())
    })?;

    tx.commit().await.map_err(|e| AppError::Internal(e.into()))?;

    let psp = state.psp.clone();
    let pool = state.pool.clone();
    let card_token = body.card_token.clone();
    let amount = invoice.total_cents;
    let business_id = auth.business_id;

    let outcome = psp.charge(&card_token, amount).await;

    match outcome {
        PspOutcome::Timeout => {
            let pool_bg = pool.clone();
            let psp_bg = state.psp.clone();
            tokio::spawn(async move {
                complete_pending_payment(pool_bg, psp_bg, attempt_id, id, business_id, card_token, amount)
                    .await;
            });
            let response = build_pay_response(
                pool.as_ref(),
                auth.business_id,
                id,
                attempt_id,
                StatusCode::ACCEPTED,
            )
            .await?;
            store_idempotency(
                pool.as_ref(),
                auth.business_id,
                &idem_key,
                &path,
                &request_hash,
                StatusCode::ACCEPTED,
                &response.1,
            )
            .await?;
            return Ok((StatusCode::ACCEPTED, Json(response.1)));
        }
        other => {
            let (status, response) = finalize_payment(
                pool.as_ref(),
                business_id,
                id,
                attempt_id,
                &card_token,
                other,
            )
            .await?;
            store_idempotency(
                pool.as_ref(),
                auth.business_id,
                &idem_key,
                &path,
                &request_hash,
                status,
                &response,
            )
            .await?;
            Ok((status, Json(response)))
        }
    }
}

async fn complete_pending_payment(
    pool: Arc<PgPool>,
    psp: Arc<PspClient>,
    attempt_id: Uuid,
    invoice_id: Uuid,
    business_id: Uuid,
    card_token: String,
    amount: i64,
) {
    let long_psp = PspClient::new(
        std::env::var("PSP_BASE_URL").unwrap_or_else(|_| "http://mock-psp:8081".into()),
        35,
    );
    let outcome = long_psp.charge(&card_token, amount).await;
    let _ = finalize_payment(&pool, business_id, invoice_id, attempt_id, &card_token, outcome).await;
    let _ = psp;
}

async fn finalize_payment(
    pool: &PgPool,
    business_id: Uuid,
    invoice_id: Uuid,
    attempt_id: Uuid,
    _card_token: &str,
    outcome: PspOutcome,
) -> AppResult<(StatusCode, PayInvoiceResponse)> {
    let mut tx = pool.begin().await.map_err(|e| AppError::Internal(e.into()))?;
    let invoice = lock_invoice(&mut tx, business_id, invoice_id).await?;

    match outcome {
        PspOutcome::Succeeded { psp_ref } => {
            if invoice.state != InvoiceState::Paid {
                invoice.state.transition_to(InvoiceState::Paid)?;
                update_invoice_state(&mut tx, invoice_id, InvoiceState::Paid).await?;
            }
            sqlx::query(
                r#"
                UPDATE payment_attempts
                SET state = 'succeeded', psp_ref = $2, updated_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(attempt_id)
            .bind(&psp_ref)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;

            tx.commit().await.map_err(|e| AppError::Internal(e.into()))?;

            let payload = serde_json::json!({
                "invoice_id": invoice_id,
                "payment_attempt_id": attempt_id,
                "psp_ref": psp_ref,
            });
            webhooks::enqueue_event(pool, business_id, "invoice.paid", payload)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;

            let response = build_pay_response(pool, business_id, invoice_id, attempt_id, StatusCode::OK)
                .await?
                .1;
            Ok((StatusCode::OK, response))
        }
        PspOutcome::Failed { code } => {
            sqlx::query(
                r#"
                UPDATE payment_attempts
                SET state = 'failed', failure_code = $2, updated_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(attempt_id)
            .bind(&code)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
            tx.commit().await.map_err(|e| AppError::Internal(e.into()))?;

            let payload = serde_json::json!({
                "invoice_id": invoice_id,
                "payment_attempt_id": attempt_id,
                "failure_code": code,
            });
            webhooks::enqueue_event(pool, business_id, "invoice.payment_failed", payload)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;

            let response =
                build_pay_response(pool, business_id, invoice_id, attempt_id, StatusCode::OK)
                    .await?
                    .1;
            Ok((StatusCode::OK, response))
        }
        PspOutcome::Timeout | PspOutcome::NetworkError => {
            sqlx::query(
                r#"
                UPDATE payment_attempts
                SET state = 'failed', failure_code = $2, updated_at = NOW()
                WHERE id = $1
                "#,
            )
            .bind(attempt_id)
            .bind(if matches!(outcome, PspOutcome::Timeout) {
                "psp_timeout"
            } else {
                "psp_network_error"
            })
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
            tx.commit().await.map_err(|e| AppError::Internal(e.into()))?;

            let code = if matches!(outcome, PspOutcome::Timeout) {
                "psp_timeout"
            } else {
                "psp_network_error"
            };
            let payload = serde_json::json!({
                "invoice_id": invoice_id,
                "payment_attempt_id": attempt_id,
                "failure_code": code,
            });
            webhooks::enqueue_event(pool, business_id, "invoice.payment_failed", payload)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;

            let response =
                build_pay_response(pool, business_id, invoice_id, attempt_id, StatusCode::OK)
                    .await?
                    .1;
            Ok((StatusCode::OK, response))
        }
    }
}

async fn build_pay_response(
    pool: &PgPool,
    business_id: Uuid,
    invoice_id: Uuid,
    attempt_id: Uuid,
    status: StatusCode,
) -> AppResult<(StatusCode, PayInvoiceResponse)> {
    let invoice = load_invoice(pool, business_id, invoice_id).await?;
    let attempt = load_payment_attempt(pool, attempt_id).await?;
    Ok((
        status,
        PayInvoiceResponse {
            payment_attempt: attempt,
            invoice,
        },
    ))
}

async fn load_payment_attempt(pool: &PgPool, id: Uuid) -> AppResult<PaymentAttemptResponse> {
    let row = sqlx::query_as::<_, AttemptRow>(
        r#"
        SELECT id, invoice_id, state::text as state, psp_ref, failure_code
        FROM payment_attempts WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(PaymentAttemptResponse {
        id: row.id,
        invoice_id: row.invoice_id,
        state: row.state,
        psp_ref: row.psp_ref,
        failure_code: row.failure_code,
    })
}

fn hash_request<T: Serialize>(body: &T) -> String {
    let bytes = serde_json::to_vec(body).expect("serialize");
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

async fn check_idempotency(
    pool: &PgPool,
    business_id: Uuid,
    key: &str,
    request_hash: &str,
) -> AppResult<Option<(StatusCode, Json<PayInvoiceResponse>)>> {
    let row = sqlx::query_as::<_, IdemRow>(
        r#"
        SELECT request_hash, response_status, response_body
        FROM idempotency_records
        WHERE business_id = $1 AND idempotency_key = $2
        "#,
    )
    .bind(business_id)
    .bind(key)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let Some(row) = row else { return Ok(None) };

    if row.request_hash != request_hash {
        return Err(AppError::Conflict(
            "idempotency key reused with different request body".into(),
        ));
    }
    let body: PayInvoiceResponse = serde_json::from_value(row.response_body)
        .map_err(|e| AppError::Internal(e.into()))?;
    let status = StatusCode::from_u16(row.response_status as u16)
        .unwrap_or(StatusCode::OK);
    Ok(Some((status, Json(body))))
}

async fn store_idempotency(
    pool: &PgPool,
    business_id: Uuid,
    key: &str,
    path: &str,
    request_hash: &str,
    status: StatusCode,
    body: &PayInvoiceResponse,
) -> AppResult<()> {
    let json = serde_json::to_value(body).map_err(|e| AppError::Internal(e.into()))?;
    sqlx::query(
        r#"
        INSERT INTO idempotency_records
            (business_id, idempotency_key, request_path, request_hash, response_status, response_body)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (business_id, idempotency_key) DO NOTHING
        "#,
    )
    .bind(business_id)
    .bind(key)
    .bind(path)
    .bind(request_hash)
    .bind(status.as_u16() as i32)
    .bind(json)
    .execute(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

struct LockedInvoice {
    state: InvoiceState,
    total_cents: i64,
}

async fn lock_invoice(
    tx: &mut Transaction<'_, Postgres>,
    business_id: Uuid,
    id: Uuid,
) -> AppResult<LockedInvoice> {
    let row = sqlx::query_as::<_, InvoiceRow>(
        r#"
        SELECT state, total_cents
        FROM invoices
        WHERE id = $1 AND business_id = $2
        FOR UPDATE
        "#,
    )
    .bind(id)
    .bind(business_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or_else(|| AppError::NotFound("invoice not found".into()))?;

    Ok(LockedInvoice {
        state: row.state,
        total_cents: row.total_cents,
    })
}

async fn update_invoice_state(
    tx: &mut Transaction<'_, Postgres>,
    id: Uuid,
    state: InvoiceState,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE invoices SET state = $2, updated_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .bind(state)
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

async fn load_invoice(pool: &PgPool, business_id: Uuid, id: Uuid) -> AppResult<InvoiceResponse> {
    let inv = sqlx::query_as::<_, InvoiceMetaRow>(
        r#"
        SELECT id, customer_id, state, total_cents, due_date, created_at, updated_at
        FROM invoices
        WHERE id = $1 AND business_id = $2
        "#,
    )
    .bind(id)
    .bind(business_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?
    .ok_or_else(|| AppError::NotFound("invoice not found".into()))?;

    let items = sqlx::query_as::<_, LineRow>(
        r#"
        SELECT description, quantity, unit_amount_cents
        FROM invoice_line_items WHERE invoice_id = $1
        "#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    Ok(InvoiceResponse {
        id: inv.id,
        customer_id: inv.customer_id,
        state: inv.state.as_str().to_string(),
        total_cents: inv.total_cents,
        due_date: inv.due_date,
        line_items: items
            .into_iter()
            .map(|l| LineItemResponse {
                description: l.description,
                quantity: l.quantity,
                unit_amount_cents: l.unit_amount_cents,
                line_total_cents: (l.quantity as i64) * l.unit_amount_cents,
            })
            .collect(),
        created_at: inv.created_at,
        updated_at: inv.updated_at,
    })
}

#[derive(sqlx::FromRow)]
struct InvoiceRow {
    state: InvoiceState,
    total_cents: i64,
}

#[derive(sqlx::FromRow)]
struct InvoiceMetaRow {
    id: Uuid,
    customer_id: Uuid,
    state: InvoiceState,
    total_cents: i64,
    due_date: NaiveDate,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct LineRow {
    description: String,
    quantity: i32,
    unit_amount_cents: i64,
}

#[derive(sqlx::FromRow)]
struct AttemptRow {
    id: Uuid,
    invoice_id: Uuid,
    state: String,
    psp_ref: Option<String>,
    failure_code: Option<String>,
}

#[derive(sqlx::FromRow)]
struct IdemRow {
    request_hash: String,
    response_status: i32,
    response_body: serde_json::Value,
}

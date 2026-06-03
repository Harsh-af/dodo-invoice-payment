use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde_json::Value;
use sha2::Sha256;
use sqlx::PgPool;
use std::time::Duration;
use tracing::{error, info, warn};
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

const RETRY_INTERVALS_SECS: &[u64] = &[1, 5, 30, 120, 600];
const MAX_ATTEMPTS: i32 = 5;

pub async fn enqueue_event(
    pool: &PgPool,
    business_id: Uuid,
    event_type: &str,
    payload: Value,
) -> anyhow::Result<()> {
    let endpoints = sqlx::query_as::<_, EndpointRow>(
        "SELECT id, url, secret FROM webhook_endpoints WHERE business_id = $1",
    )
    .bind(business_id)
    .fetch_all(pool)
    .await?;

    for ep in endpoints {
        sqlx::query(
            r#"
            INSERT INTO webhook_deliveries (endpoint_id, event_type, payload)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(ep.id)
        .bind(event_type)
        .bind(&payload)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub fn spawn_delivery_worker(pool: PgPool, http: Client) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = process_batch(&pool, &http).await {
                error!(?e, "webhook worker batch failed");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
}

async fn process_batch(pool: &PgPool, http: &Client) -> anyhow::Result<()> {
    let rows = sqlx::query_as::<_, DeliveryRow>(
        r#"
        SELECT d.id, d.endpoint_id, d.event_type, d.payload, d.attempt_count,
               e.url, e.secret
        FROM webhook_deliveries d
        JOIN webhook_endpoints e ON e.id = d.endpoint_id
        WHERE d.status = 'pending' AND d.next_retry_at <= NOW()
        ORDER BY d.next_retry_at
        LIMIT 20
        FOR UPDATE SKIP LOCKED
        "#,
    )
    .fetch_all(pool)
    .await?;

    for row in rows {
        deliver_one(pool, http, row).await?;
    }
    Ok(())
}

async fn deliver_one(pool: &PgPool, http: &Client, row: DeliveryRow) -> anyhow::Result<()> {
    let timestamp = Utc::now().timestamp();
    let body = serde_json::json!({
        "id": row.id,
        "type": row.event_type,
        "created_at": Utc::now().to_rfc3339(),
        "data": row.payload,
    });
    let body_str = body.to_string();
    let signed_payload = format!("{timestamp}.{body_str}");
    let signature = sign(&row.secret, &signed_payload);

    let resp = http
        .post(&row.url)
        .header("X-Dodo-Timestamp", timestamp.to_string())
        .header("X-Dodo-Signature", format!("v1={signature}"))
        .header("X-Dodo-Event", &row.event_type)
        .header("Content-Type", "application/json")
        .body(body_str)
        .timeout(Duration::from_secs(10))
        .send()
        .await;

    let attempt = row.attempt_count + 1;
    match resp {
        Ok(r) if r.status().is_success() => {
            sqlx::query(
                "UPDATE webhook_deliveries SET status = 'delivered', attempt_count = $2 WHERE id = $1",
            )
            .bind(row.id)
            .bind(attempt)
            .execute(pool)
            .await?;
            info!(delivery_id = %row.id, "webhook delivered");
        }
        Ok(r) => {
            warn!(delivery_id = %row.id, status = %r.status(), "webhook delivery failed");
            schedule_retry(pool, row.id, attempt, Some(format!("http {}", r.status()))).await?;
        }
        Err(e) => {
            warn!(delivery_id = %row.id, ?e, "webhook delivery error");
            schedule_retry(pool, row.id, attempt, Some(e.to_string())).await?;
        }
    }
    Ok(())
}

async fn schedule_retry(
    pool: &PgPool,
    id: Uuid,
    attempt: i32,
    err: Option<String>,
) -> anyhow::Result<()> {
    if attempt >= MAX_ATTEMPTS {
        sqlx::query(
            r#"
            UPDATE webhook_deliveries
            SET status = 'exhausted', attempt_count = $2, last_error = $3
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(attempt)
        .bind(err)
        .execute(pool)
        .await?;
        return Ok(());
    }
    let idx = (attempt as usize).saturating_sub(1).min(RETRY_INTERVALS_SECS.len() - 1);
    let delay = RETRY_INTERVALS_SECS[idx];
    sqlx::query(
        r#"
        UPDATE webhook_deliveries
        SET attempt_count = $2,
            next_retry_at = NOW() + ($3::text || ' seconds')::interval,
            last_error = $4
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(attempt)
    .bind(delay.to_string())
    .bind(err)
    .execute(pool)
    .await?;
    Ok(())
}

pub fn sign(secret: &str, payload: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(payload.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

#[derive(sqlx::FromRow)]
struct EndpointRow {
    id: Uuid,
    url: String,
    secret: String,
}

#[derive(sqlx::FromRow)]
struct DeliveryRow {
    id: Uuid,
    endpoint_id: Uuid,
    event_type: String,
    payload: Value,
    attempt_count: i32,
    url: String,
    secret: String,
}

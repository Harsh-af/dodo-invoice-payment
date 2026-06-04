use crate::auth::{api_key_prefix, extract_auth_lookup, generate_api_key};
use sqlx::PgPool;
use tracing::info;

const DEMO_BUSINESS_NAME: &str = "Demo Business";

pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM information_schema.tables
            WHERE table_schema = 'public' AND table_name = 'businesses'
        )
        "#,
    )
    .fetch_one(pool)
    .await?;

    if !exists {
        let sql = include_str!("../../migrations/001_init.sql");
        sqlx::raw_sql(sql).execute(pool).await?;
    }

    migrate_idempotency_scope(pool).await?;
    Ok(())
}

/// Idempotency cache is scoped per pay URL, not globally per business + key.
async fn migrate_idempotency_scope(pool: &PgPool) -> anyhow::Result<()> {
    let scoped: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM pg_constraint c
            JOIN pg_class t ON t.oid = c.conrelid
            WHERE t.relname = 'idempotency_records'
              AND c.contype = 'u'
              AND pg_get_constraintdef(c.oid) LIKE '%request_path%'
        )
        "#,
    )
    .fetch_one(pool)
    .await?;

    if scoped {
        return Ok(());
    }

    sqlx::raw_sql(
        r#"
        ALTER TABLE idempotency_records
            DROP CONSTRAINT IF EXISTS idempotency_records_business_id_idempotency_key_key;
        ALTER TABLE idempotency_records
            ADD CONSTRAINT idempotency_records_business_id_idempotency_key_request_path_key
            UNIQUE (business_id, idempotency_key, request_path);
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

/// Called on every startup before the server accepts traffic.
/// With `DEMO_API_KEY` set (docker compose default), creates schema data on a fresh DB
/// and keeps the documented key in sync across restarts.
pub async fn seed_demo(pool: &PgPool, demo_key: Option<String>) -> anyhow::Result<Option<String>> {
    let Some(demo_key) = demo_key.filter(|k| !k.is_empty()) else {
        return seed_ephemeral_key_if_empty(pool).await;
    };

    let business_id = ensure_demo_business(pool).await?;
    upsert_api_key(pool, business_id, &demo_key).await?;
    log_demo_key(&demo_key);
    assert_demo_key_works(pool, &demo_key).await?;
    Ok(Some(demo_key))
}

async fn ensure_demo_business(pool: &PgPool) -> anyhow::Result<uuid::Uuid> {
    if let Some(id) = sqlx::query_scalar(
        "SELECT id FROM businesses WHERE name = $1 LIMIT 1",
    )
    .bind(DEMO_BUSINESS_NAME)
    .fetch_optional(pool)
    .await?
    {
        return Ok(id);
    }

    sqlx::query_scalar("INSERT INTO businesses (name) VALUES ($1) RETURNING id")
        .bind(DEMO_BUSINESS_NAME)
        .fetch_one(pool)
        .await
        .map_err(Into::into)
}

async fn upsert_api_key(pool: &PgPool, business_id: uuid::Uuid, full_key: &str) -> anyhow::Result<()> {
    let prefix = api_key_prefix(full_key);
    let hash = bcrypt::hash(full_key, 12)?;

    let has_active_key: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM api_keys
            WHERE business_id = $1 AND revoked_at IS NULL
        )
        "#,
    )
    .bind(business_id)
    .fetch_one(pool)
    .await?;

    if has_active_key {
        sqlx::query(
            r#"
            UPDATE api_keys
            SET key_prefix = $1, key_hash = $2, revoked_at = NULL
            WHERE business_id = $3 AND revoked_at IS NULL
            "#,
        )
        .bind(&prefix)
        .bind(&hash)
        .bind(business_id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO api_keys (business_id, key_prefix, key_hash) VALUES ($1, $2, $3)",
        )
        .bind(business_id)
        .bind(&prefix)
        .bind(&hash)
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn assert_demo_key_works(pool: &PgPool, full_key: &str) -> anyhow::Result<()> {
    extract_auth_lookup(pool, full_key)
        .await
        .map_err(|e| anyhow::anyhow!("demo API key failed auth check after seed: {e}"))?;
    Ok(())
}

async fn seed_ephemeral_key_if_empty(pool: &PgPool) -> anyhow::Result<Option<String>> {
    let key_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM api_keys")
        .fetch_one(pool)
        .await?;

    if key_count > 0 {
        return Ok(None);
    }

    let business_id = ensure_demo_business(pool).await?;
    let (full, prefix, hash) = generate_api_key();
    sqlx::query(
        "INSERT INTO api_keys (business_id, key_prefix, key_hash) VALUES ($1, $2, $3)",
    )
    .bind(business_id)
    .bind(&prefix)
    .bind(&hash)
    .execute(pool)
    .await?;

    log_demo_key(&full);
    Ok(Some(full))
}

fn log_demo_key(full_key: &str) {
    info!("============================================================");
    info!("Demo API key (use in Authorization: Bearer <key>):");
    info!("{full_key}");
    info!("============================================================");
}

pub async fn register_demo_webhook(pool: &PgPool) -> anyhow::Result<()> {
    let business_id: Option<uuid::Uuid> = sqlx::query_scalar(
        "SELECT id FROM businesses WHERE name = $1 LIMIT 1",
    )
    .bind(DEMO_BUSINESS_NAME)
    .fetch_optional(pool)
    .await?;

    let Some(business_id) = business_id else {
        return Ok(());
    };

    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM webhook_endpoints WHERE business_id = $1)",
    )
    .bind(business_id)
    .fetch_one(pool)
    .await?;

    if exists {
        return Ok(());
    }

    sqlx::query(
        r#"
        INSERT INTO webhook_endpoints (business_id, url, secret)
        VALUES ($1, 'http://webhook-receiver:8090/webhook', 'demo-endpoint-secret')
        "#,
    )
    .bind(business_id)
    .execute(pool)
    .await?;

    Ok(())
}

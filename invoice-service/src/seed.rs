use crate::auth::generate_api_key;
use sqlx::PgPool;
use tracing::info;

pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    let sql = include_str!("../../migrations/001_init.sql");
    sqlx::raw_sql(sql).execute(pool).await?;
    Ok(())
}

pub async fn seed_demo(pool: &PgPool, demo_key: Option<String>) -> anyhow::Result<Option<String>> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM businesses")
        .fetch_one(pool)
        .await?;

    if count > 0 {
        return Ok(demo_key);
    }

    let business_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO businesses (name) VALUES ('Demo Business') RETURNING id",
    )
    .fetch_one(pool)
    .await?;

    let (full_key, prefix, hash) = if let Some(k) = demo_key.clone() {
        let prefix = k[..12.min(k.len())].to_string();
        let hash = bcrypt::hash(&k, 12)?;
        (k, prefix, hash)
    } else {
        generate_api_key()
    };

    sqlx::query(
        "INSERT INTO api_keys (business_id, key_prefix, key_hash) VALUES ($1, $2, $3)",
    )
    .bind(business_id)
    .bind(&prefix)
    .bind(&hash)
    .execute(pool)
    .await?;

    info!("============================================================");
    info!("Demo API key (use in Authorization: Bearer <key>):");
    info!("{}", full_key);
    info!("============================================================");

    Ok(Some(full_key))
}

pub async fn register_demo_webhook(pool: &PgPool) -> anyhow::Result<()> {
    let business_id: Option<uuid::Uuid> =
        sqlx::query_scalar("SELECT id FROM businesses LIMIT 1")
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

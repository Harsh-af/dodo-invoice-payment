mod app_state;
mod auth;
mod config;
mod error;
mod psp;
mod routes;
mod seed;
mod state_machine;
mod webhooks;

use app_state::AppState;
use auth::auth_middleware;
use axum::{
    middleware,
    routing::{get, post},
    Router,
};
use config::Config;
use psp::PspClient;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "invoice_service=info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env();
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    seed::run_migrations(&pool).await?;
    seed::seed_demo(&pool, config.demo_api_key.clone()).await?;

    let psp = Arc::new(PspClient::new(
        config.psp_base_url.clone(),
        config.psp_timeout_secs,
    ));

    let invoice_state = Arc::new(AppState {
        pool: Arc::new(pool.clone()),
        psp,
    });

    let http = reqwest::Client::new();
    webhooks::spawn_delivery_worker(pool.clone(), http);

    seed::register_demo_webhook(&pool).await?;

    let protected = Router::new()
        .route("/customers", post(routes::customers::create_customer).get(routes::customers::list_customers))
        .route(
            "/customers/:id",
            get(routes::customers::get_customer),
        )
        .route("/invoices", post(routes::invoices::create_invoice).get(routes::invoices::list_invoices))
        .route("/invoices/:id", get(routes::invoices::get_invoice))
        .route(
            "/invoices/:id/finalize",
            post(routes::invoices::finalize_invoice),
        )
        .route("/invoices/:id/void", post(routes::invoices::void_invoice))
        .route("/invoices/:id/pay", post(routes::invoices::pay_invoice))
        .route("/webhooks/endpoints", post(routes::webhooks::register_webhook))
        .layer(middleware::from_fn_with_state(
            invoice_state.clone(),
            auth_middleware,
        ))
        .with_state(invoice_state);

    let app = Router::new()
        .route("/health", get(routes::health::health))
        .merge(protected)
        .layer(TraceLayer::new_for_http());

    info!("invoice service listening on {}", config.listen_addr);
    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

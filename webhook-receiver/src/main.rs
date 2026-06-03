use axum::{
    body::to_bytes,
    extract::Request,
    routing::post,
    Router,
};
use tracing::info;

async fn receive(req: Request) -> &'static str {
    let (parts, body) = req.into_parts();
    let headers = parts.headers;
    let sig = headers
        .get("X-Dodo-Signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-");
    let event = headers
        .get("X-Dodo-Event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("-");
    let body = to_bytes(body, 1024 * 1024).await.unwrap_or_default();
    info!(
        event = %event,
        signature = %sig,
        body = %String::from_utf8_lossy(&body),
        "webhook received"
    );
    "ok"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();
    let app = Router::new()
        .route("/", post(receive))
        .route("/webhook", post(receive));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8090")
        .await
        .unwrap();
    info!("webhook receiver on :8090");
    axum::serve(listener, app).await.unwrap();
}

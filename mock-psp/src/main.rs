use axum::{
    extract::State,
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tower_http::trace::TraceLayer;
use tracing::info;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    drop_connections: bool,
}

#[derive(Debug, Deserialize)]
struct ChargeRequest {
    card_token: String,
    amount_cents: i64,
}

#[derive(Debug, Serialize)]
struct ChargeResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    psp_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}

async fn charge(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ChargeRequest>,
) -> Result<Json<ChargeResponse>, StatusCode> {
    info!(token = %body.card_token, amount = body.amount_cents, "psp charge");

    if body.card_token == "tok_network_error" && state.drop_connections {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    sleep(Duration::from_millis(100)).await;

    match body.card_token.as_str() {
        "tok_success" => Ok(Json(ChargeResponse {
            status: "succeeded".into(),
            psp_ref: Some(Uuid::new_v4().to_string()),
            code: None,
        })),
        "tok_insufficient_funds" => Ok(Json(ChargeResponse {
            status: "failed".into(),
            psp_ref: None,
            code: Some("insufficient_funds".into()),
        })),
        "tok_card_declined" => Ok(Json(ChargeResponse {
            status: "failed".into(),
            psp_ref: None,
            code: Some("card_declined".into()),
        })),
        "tok_timeout" => {
            sleep(Duration::from_secs(30)).await;
            Ok(Json(ChargeResponse {
                status: "succeeded".into(),
                psp_ref: Some(Uuid::new_v4().to_string()),
                code: None,
            }))
        }
        "tok_network_error" => Err(StatusCode::INTERNAL_SERVER_ERROR),
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("mock_psp=info,tower_http=info")
        .init();

    let drop = std::env::var("PSP_DROP_CONNECTION")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let state = Arc::new(AppState {
        drop_connections: drop,
    });

    let app = Router::new()
        .route("/charge", post(charge))
        .route("/health", axum::routing::get(|| async { "ok" }))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    let addr = "0.0.0.0:8081";
    info!("mock PSP listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

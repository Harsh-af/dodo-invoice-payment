use crate::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Serialize)]
pub struct PspChargeRequest {
    pub card_token: String,
    pub amount_cents: i64,
}

#[derive(Debug, Deserialize)]
pub struct PspChargeResponse {
    pub status: String,
    pub psp_ref: Option<String>,
    pub code: Option<String>,
}

pub enum PspOutcome {
    Succeeded { psp_ref: String },
    Failed { code: String },
    Timeout,
    NetworkError,
}

pub struct PspClient {
    client: Client,
    base_url: String,
}

impl PspClient {
    pub fn new(base_url: String, timeout_secs: u64) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .expect("http client");
        Self { client, base_url }
    }

    pub async fn charge(&self, card_token: &str, amount_cents: i64) -> PspOutcome {
        let url = format!("{}/charge", self.base_url.trim_end_matches('/'));
        let body = PspChargeRequest {
            card_token: card_token.to_string(),
            amount_cents,
        };

        let resp = match self.client.post(&url).json(&body).send().await {
            Ok(r) => r,
            Err(e) if e.is_timeout() => return PspOutcome::Timeout,
            Err(_) => return PspOutcome::NetworkError,
        };

        if !resp.status().is_success() {
            return PspOutcome::NetworkError;
        }

        let parsed: PspChargeResponse = match resp.json().await {
            Ok(p) => p,
            Err(_) => return PspOutcome::NetworkError,
        };

        match parsed.status.as_str() {
            "succeeded" => PspOutcome::Succeeded {
                psp_ref: parsed.psp_ref.unwrap_or_else(|| "unknown".into()),
            },
            "failed" => PspOutcome::Failed {
                code: parsed.code.unwrap_or_else(|| "unknown".into()),
            },
            _ => PspOutcome::NetworkError,
        }
    }
}

pub fn card_token_hint(token: &str) -> String {
    if token.len() <= 8 {
        token.to_string()
    } else {
        format!("{}…", &token[..8])
    }
}

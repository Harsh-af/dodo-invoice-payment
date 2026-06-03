//! Run with services up: `docker compose up -d` then
//! `INTEGRATION_TEST=1 cargo test -p invoice-service --test integration -- --nocapture`

use reqwest::{Client, StatusCode};
use serde_json::json;
use std::env;
use std::time::Duration;
use uuid::Uuid;

const API_KEY: &str = "dodo_sk_demo_key_for_assignment_only";
const BASE: &str = "http://127.0.0.1:8080";

fn enabled() -> bool {
    env::var("INTEGRATION_TEST").ok().as_deref() == Some("1")
}

async fn client() -> Client {
    Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap()
}

async fn create_open_invoice(c: &Client) -> (Uuid, Uuid) {
    let customer: serde_json::Value = c
        .post(format!("{BASE}/customers"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({"name": "Test", "email": format!("t-{}@example.com", Uuid::new_v4())}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let customer_id = customer["id"].as_str().unwrap();

    let invoice: serde_json::Value = c
        .post(format!("{BASE}/invoices"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .json(&json!({
            "customer_id": customer_id,
            "due_date": "2026-12-31",
            "finalize": true,
            "line_items": [{
                "description": "Widget",
                "quantity": 2,
                "unit_amount_cents": 1500
            }]
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    (
        invoice["id"].as_str().unwrap().parse().unwrap(),
        customer["id"].as_str().unwrap().parse().unwrap(),
    )
}

#[tokio::test]
async fn concurrent_pay_at_most_one_charge() {
    if !enabled() {
        return;
    }
    let c = client().await;
    let (invoice_id, _) = create_open_invoice(&c).await;
    let n = 8usize;
    let mut handles = Vec::new();
    for i in 0..n {
        let client = c.clone();
        handles.push(tokio::spawn(async move {
            client
                .post(format!("{BASE}/invoices/{invoice_id}/pay"))
                .header("Authorization", format!("Bearer {API_KEY}"))
                .header("Idempotency-Key", format!("concurrent-{i}"))
                .json(&json!({"card_token": "tok_success"}))
                .send()
                .await
        }));
    }
    let mut ok = 0;
    let mut conflict = 0;
    for h in handles {
        let resp = h.await.unwrap().unwrap();
        if resp.status() == StatusCode::OK {
            ok += 1;
        } else if resp.status() == StatusCode::CONFLICT || resp.status() == StatusCode::ACCEPTED {
            conflict += 1;
        }
    }
    let invoice: serde_json::Value = c
        .get(format!("{BASE}/invoices/{invoice_id}"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(invoice["state"], "paid");
    assert!(ok >= 1, "expected at least one successful payment");
    assert!(ok <= 1, "expected at most one successful payment, got {ok}");
    assert!(ok + conflict >= n);
}

#[tokio::test]
async fn idempotency_replay_no_duplicate_attempts() {
    if !enabled() {
        return;
    }
    let c = client().await;
    let (invoice_id, _) = create_open_invoice(&c).await;
    let key = format!("idem-{}", Uuid::new_v4());
    let body = json!({"card_token": "tok_success"});

    let r1 = c
        .post(format!("{BASE}/invoices/{invoice_id}/pay"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .header("Idempotency-Key", &key)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status(), StatusCode::OK);
    let p1: serde_json::Value = r1.json().await.unwrap();

    let r2 = c
        .post(format!("{BASE}/invoices/{invoice_id}/pay"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .header("Idempotency-Key", &key)
        .json(&body)
        .send()
        .await
        .unwrap();
    assert_eq!(r2.status(), StatusCode::OK);
    let p2: serde_json::Value = r2.json().await.unwrap();

    assert_eq!(p1["payment_attempt"]["id"], p2["payment_attempt"]["id"]);
    assert_eq!(p1["invoice"]["state"], "paid");
}

#[tokio::test]
async fn psp_network_error_leaves_invoice_open() {
    if !enabled() {
        return;
    }
    let c = client().await;
    let (invoice_id, _) = create_open_invoice(&c).await;

    let resp = c
        .post(format!("{BASE}/invoices/{invoice_id}/pay"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .header("Idempotency-Key", format!("net-{}", Uuid::new_v4()))
        .json(&json!({"card_token": "tok_network_error"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let invoice: serde_json::Value = c
        .get(format!("{BASE}/invoices/{invoice_id}"))
        .header("Authorization", format!("Bearer {API_KEY}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(invoice["state"], "open");
    assert_eq!(
        resp.json::<serde_json::Value>().await.unwrap()["payment_attempt"]["state"],
        "failed"
    );
}

# Dodo Invoice & Payment Service (Take-Home)

Minimal invoice and payment API in **Rust (Axum)** with PostgreSQL, a mock PSP, signed webhooks, and one-command Docker setup.

## Quick start

```bash
docker compose up --build
```

Wait until logs show the demo API key. Default key (also in compose):

```
dodo_sk_demo_key_for_assignment_only
```

Base URL: `http://localhost:8080`

## Demo Video

Record your 5–10 minute Loom (or equivalent) and paste the link here:

```
TODO: https://www.loom.com/share/your-video-id
```

## curl examples

Set variables:

```bash
export API_KEY="dodo_sk_demo_key_for_assignment_only"
export BASE="http://localhost:8080"
```

### 1. Create customer

```bash
curl -s -X POST "$BASE/customers" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"name":"Ada Lovelace","email":"ada@example.com"}'
```

### 2. Create invoice (open, ready to pay)

```bash
curl -s -X POST "$BASE/invoices" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "customer_id": "<CUSTOMER_UUID>",
    "due_date": "2026-12-31",
    "finalize": true,
    "line_items": [
      {"description": "Consulting", "quantity": 2, "unit_amount_cents": 5000}
    ]
  }'
```

Server computes `total_cents` = 10000.

### 3. Pay successfully

```bash
curl -s -X POST "$BASE/invoices/<INVOICE_UUID>/pay" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: pay-success-1" \
  -d '{"card_token":"tok_success"}'
```

### 4. Pay declined

```bash
curl -s -X POST "$BASE/invoices/<INVOICE_UUID>/pay" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: pay-decline-1" \
  -d '{"card_token":"tok_card_declined"}'
```

Invoice stays `open`; webhook `invoice.payment_failed` is enqueued. Watch webhook receiver logs: `docker compose logs -f webhook-receiver`.

## Integration tests

With stack running:

```bash
set INTEGRATION_TEST=1
cargo test -p invoice-service --test integration
```

Requires Rust locally, or run inside a Rust container. Tests cover concurrent pay, idempotency replay, and `tok_network_error`.

## Project layout

| Path | Description |
|------|-------------|
| `invoice-service/` | Main API |
| `mock-psp/` | Mock payment processor (`:8081`) |
| `webhook-receiver/` | Logs inbound webhooks (`:8090`) |
| `migrations/` | PostgreSQL schema |
| `DESIGN.md` | Primary design deliverable |
| `docs/openapi.yaml` | API spec |

## Language choice

Rust (Axum) per assignment preference. All services build in Docker without a local Rust install.

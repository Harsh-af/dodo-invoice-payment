# Dodo Invoice & Payment Service (Take-Home)

Minimal invoice and payment API in **Rust (Axum)** with PostgreSQL, a mock PSP, signed webhooks, and one-command Docker setup.

## Quick start

```bash
docker compose up --build
```

On a **fresh database** (first `docker compose up`), the service applies migrations and seeds a demo business, API key, and webhook endpoint automatically. No manual DB setup is required.

Wait until `invoice-service` logs show the demo API key. Default key (also in `docker-compose.yml` as `DEMO_API_KEY`):

```
dodo_demo_key
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
export API_KEY="dodo_demo_key"
export BASE="http://localhost:8080"
```

## Watch webhook events:
```bash
docker compose logs -f webhook-receiver
```

### 1. Create customer

```bash
curl -s -X POST "$BASE/customers" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"name":"Harsh Karanwal","email":"harshkaranwal@gmail.com"}'
```

### 2. Create invoice (open, ready to pay)

```bash
curl -s -X POST "$BASE/invoices" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "customer_id": "dea025f6-4c5e-4224-822f-8f3141d42aba",
    "due_date": "2026-12-31",
    "finalize": true,
    "line_items": [
      {"description": "GTA 6", "quantity": 6, "unit_amount_cents": 2024}
    ]
  }'
```

Server computes `total_cents` = 6048

### 3. Pay successfully

```bash
curl -s -X POST "$BASE/invoices/0403031e-010f-4080-a940-e0a85466322b/pay" \
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
INTEGRATION_TEST=1 cargo test --all
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

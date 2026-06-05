# Dodo Invoice & Payment Service

Invoice and payment API in **Rust (Axum)** with PostgreSQL, a mock PSP, signed webhooks, and the one-command Docker setup.

## Demo Video
The demo is 17 minutes (slightly above the recommended duration). It covers all required sections from the assignment specification.
- Video URL : [Google Drive](https://drive.google.com/file/d/1FLFwfWsJf7jVPjQ72LOY9ejzny4tc6no/view?usp=sharing)

One thing I realized after recording was that I forgot to show the live webhook request logs during the demo. So I have included screenshots of the webhook delivery logs below:
1. Image URL: [Docker Image server logs](https://drive.google.com/file/d/1V-ADPpjmC0CUlAXDl8TsGeSz8OoFs0GF/view?usp=sharing) <br/>
2. Image URL: [DB logs](https://drive.google.com/file/d/1CNR-VwSOMVYf0GGZnNcNtZdiUmqjZA-p/view?usp=sharing)

## Quick start

### Clone the git
```bash
git clone https://github.com/Harsh-af/dodo-invoice-payment.git
```

### Docker up+build
```bash
docker compose up --build
```

### Access Details:
dodo_demo_key <br/>
Base URL: `http://localhost:8080`

<br/>

## curl commands(directly in the bash)

Set variables:

```bash
export API_KEY="dodo_demo_key"
export BASE="http://localhost:8080"
```

## Watch webhook events(in a different bash tab):
```bash
docker compose logs -f webhook-receiver
```

<br/>

## API Examples

### Create customer

```bash
curl -s -X POST "$BASE/customers" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"name":"Harsh Karanwal","email":"harshkaranwal@gmail.com"}'
```

### Create Invoice

```bash
curl -s -X POST "$BASE/invoices" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "customer_id": "<CUSTOMER_UUID>",
    "due_date": "2026-12-31",
    "finalize": true,
    "line_items": [
      {"description": "GTA 6", "quantity": 2, "unit_amount_cents": 1423}
    ]
  }'
```

<br/>

## Payment Cases

### Payment Success

```bash
curl -s -X POST "$BASE/invoices/<INVOICE_UUID>/pay" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: pay-success-1" \
  -d '{"card_token":"tok_success"}'
```

### Payment declined

```bash
curl -s -X POST "$BASE/invoices/<INVOICE_UUID>/pay" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: pay-decline-1" \
  -d '{"card_token":"tok_card_declined"}'
```

### Concurrent Payment Test

```bash
curl -s -X POST "$BASE/invoices/<INVOICE_UUID>/pay" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: test-1" \
  -d '{"card_token":"tok_success"}' &

curl -s -X POST "$BASE/invoices/<INVOICE_UUID>/pay" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: test-2" \
  -d '{"card_token":"tok_success"}' &

wait
```

### tok_timeout

```bash
export INVOICE_ID="<INVOICE_UUID>"

time curl -s -w "\nHTTP %{http_code}\n" -X POST "$BASE/invoices/$INVOICE_ID/pay" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: timeout-$(uuidgen)" \
  -d '{"card_token":"tok_timeout"}'
```

### tok_network_error

```bash
curl -s -X POST "$BASE/invoices/<INVOICE_UUID>/pay" \
  -H "Authorization: Bearer $API_KEY" \
  -H "Content-Type: application/json" \
  -H "Idempotency-Key: net-error-$(date +%s)" \
  -d '{"card_token":"tok_network_error"}'
```

<br/>

## Project Structure

| Path | Description |
|------|-------------|
| `invoice-service/` | Core API service |
| `mock-psp/` | Mock payment processor (`:8081`) |
| `webhook-receiver/` | Logs webhook deliveries (`:8090`) |
| `migrations/` | PostgreSQL schema |
| `DESIGN.md` | System design document |
| `docs/openapi.yaml` | API specification |

---

## Tech Stack

- Rust (Axum)
- PostgreSQL
- Docker / Docker Compose
- Mock PSP service
- Webhook signing (HMAC-SHA256)

---

* All services are fully containerized and require no local Rust installation.

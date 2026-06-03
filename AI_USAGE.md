# AI Usage Disclosure

## Tools used

- **Cursor (Claude)** — Primary assistant for scaffolding the Rust/Axum workspace, docker-compose layout, SQL migrations, and drafting `DESIGN.md` / `README.md` / OpenAPI from the assignment PDF.
- **Cursor autocomplete** — Boilerplate for serde structs and SQLx query shapes.

## Three decisions made independently of AI

1. **Row-level `FOR UPDATE` + pending-attempt guard (not serializable isolation)**  
   AI suggested advisory locks as an option. I chose row locks because contention is per-invoice, the behavior is easy to demonstrate in a concurrency test, and we avoid database-wide serialization rollbacks.

2. **202 Accepted + background completion for `tok_timeout`**  
   AI initially leaned toward returning 504 Gateway Timeout. I rejected that because it forces clients to treat timeout as failure while the PSP may still succeed. Returning 202 with a `pending` attempt matches async payment UX and keeps the HTTP handler under 5 seconds.

3. **Webhook outbox table with polling worker**  
   AI mentioned firing-and-forgetting `tokio::spawn` per event. I chose a durable outbox so retries survive process restarts and backoff is visible in SQL for debugging.

## One thing AI got wrong (and how I verified)

AI placed `AuthContext` as a separate Axum `State` alongside `AppState`, which does not compile — Axum allows one `State` type per router branch. I merged auth into `AppState` and verified by building the Docker image (`docker compose build`). I also manually traced the pay flow to ensure idempotency is checked before any PSP HTTP call.

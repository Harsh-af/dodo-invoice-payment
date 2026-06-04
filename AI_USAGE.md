# AI Usage Disclosure

## Tools used and for what
- **Cursor (Opus 4.8/Auto)**
  - Assistanece with building the Rust/Axum workspace
  - handler, models and routing
  - docker-compose layout was desined and configured by me, polished by Cursor AI
  - SQL migrations
  - Polishing `DESIGN.md` / `README.md` / OpenAPI according to the assignment PDF.
- **Cursor autocomplete** - Boilerplate for serde structs and SQLx query shapes.
- ChatGPT - mostly for rubber ducking
  - To cross question my own Scrum on the environmenbt for this project

## Three decisions made independently of AI

1. **Row-level `FOR UPDATE` + pending-attempt guard (not serializable isolation)**  
   AI suggested advisory locks as an option. I chose row locks because contention is per-invoice, the behavior is easy to demonstrate in a concurrency test, and we avoid database-wide serialization rollbacks.

2. **202 Accepted + background completion for `tok_timeout`**  
   AI initially leaned toward returning 504 Gateway Timeout. I rejected that because it forces clients to treat timeout as failure while the PSP may still succeed. Returning 202 with a `pending` attempt matches async payment UX and keeps the HTTP handler under 5 seconds.

3. **Webhook outbox table with polling worker**  
   AI mentioned firing-and-forgetting `tokio::spawn` per event. I chose a durable outbox so retries survive process restarts and backoff is visible in SQL for debugging.

## One thing AI got wrong (and how I verified)

#### API key hashing: bcrypt instead of SHA-256
AI suggested storing API keys using SHA-256 hashing. I chose bcrypt instead because API keys are credentials and benefit from a deliberately slow password-hashing algorithm. This makes brute-force attacks significantly more expensive if the database is ever leaked. The performance impact is negligible because API keys are verified infrequently compared to normal application queries.

AI placed `AuthContext` as a separate Axum `State` alongside `AppState`, which does not compile - Axum allows one `State` type per router branch. I merged auth into `AppState` and verified by building the Docker image (`docker compose build`). I also manually traced the pay flow to ensure idempotency is checked before any PSP HTTP call.

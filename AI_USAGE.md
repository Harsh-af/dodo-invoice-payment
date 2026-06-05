# AI Usage Disclosure

## Tools Used and Purpose

- **Cursor (Opus 4.8/Auto)**
  - Assistanece with building the Rust/Axum workspace
  - handler, models and routing
  - docker-compose layout was desined and configured by me, polished by Cursor AI
  - SQL migrations
  - Polishing `DESIGN.md`, `README.md` and OpenAPI specification based on assignment requirements

- **Cursor autocomplete** - Boilerplate for serde structs and SQLx query shapes.

- **ChatGPT**
  - Mostly for rubber ducking
  - To cross question my own scrum on the environmenbt for this project

---

## Three decisions made independently of AI

1. **Row-level `FOR UPDATE` + pending-attempt guard (instead of serializable isolation)**  
AI suggested advisory locks and higher isolation levels. I used row-level locking because contention is scoped per invoice, and it keeps concurrency behavior deterministic and easy to validate in tests without introducing global transaction conflicts.

2. **Per-scope idempotency design `(business_id, request_path, idempotency_key)`**  
Instead of a single global idempotency namespace, I scoped keys by tenant and endpoint. This prevents accidental cross-endpoint collisions and keeps lookup logic simple and indexed at the database level.

3. **Webhook outbox table instead of in-memory async delivery**  
AI suggested spawning background tasks per event. I used a persistent outbox so webhook delivery survives process restarts and retries are fully traceable in the database, which is important for debugging and failure recovery.

## One thing AI got wrong (and how I verified)

1. **API key hashing approach (bcrypt instead of SHA-256)** <br/> AI suggested storing API keys using SHA-256 hashing. I chose bcrypt instead because API keys are credentials and benefit from a deliberately slow password-hashing algorithm. This makes brute-force attacks significantly more expensive if the database is ever leaked. The performance impact is negligible because API keys are verified infrequently compared to normal application queries.

2. **Axum state design structure** <br/> AI initially separated authentication context from application state, which is invalid in Axum due to single-state router constraints. I consolidated it into a single `AppState` and validated the fix by rebuilding the service (`docker compose build`) and running integration flows.

use crate::psp::PspClient;
use sqlx::PgPool;
use std::sync::Arc;

pub struct AppState {
    pub pool: Arc<PgPool>,
    pub psp: Arc<PspClient>,
}

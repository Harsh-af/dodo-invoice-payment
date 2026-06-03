pub struct Config {
    pub database_url: String,
    pub psp_base_url: String,
    pub psp_timeout_secs: u64,
    pub listen_addr: String,
    pub webhook_signing_secret: String,
    pub demo_api_key: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://invoice:invoice@localhost:5432/invoice".into()),
            psp_base_url: std::env::var("PSP_BASE_URL")
                .unwrap_or_else(|_| "http://localhost:8081".into()),
            psp_timeout_secs: std::env::var("PSP_TIMEOUT_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            listen_addr: std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into()),
            webhook_signing_secret: std::env::var("WEBHOOK_SIGNING_SECRET")
                .unwrap_or_else(|_| "dev-webhook-secret-change-in-prod".into()),
            demo_api_key: std::env::var("DEMO_API_KEY").ok(),
        }
    }
}

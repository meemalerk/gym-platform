//! Configuration from the environment.
//!
//! Secrets come from the deployment environment (or a secret manager) — `.env` is
//! for local development only and is git-ignored.

use anyhow::{Context, Result, bail};

/// The placeholder shipped in `.env.example`. Refusing to boot with it outside
/// development turns "we forgot to set the secret" from a silent, exploitable
/// misconfiguration into a loud startup failure.
const INSECURE_DEV_SECRET: &str = "dev-only-insecure-secret-change-me-before-any-real-deployment";

const MIN_SECRET_LEN: usize = 32;

#[derive(Debug, Clone)]
pub struct Config {
    /// Privileged connection, used **only** to apply migrations at boot.
    pub database_url: String,
    /// Runtime connection, using the unprivileged `gym_app` role.
    ///
    /// This must NOT be the table owner: Postgres bypasses row-level security for
    /// table owners and superusers, so connecting as the owner silently disables
    /// every policy. Falls back to `database_url` with a loud warning.
    pub app_database_url: String,
    pub database_max_connections: u32,
    pub host: String,
    pub port: u16,
    pub jwt_secret: String,
    pub access_token_ttl_seconds: i64,
    pub refresh_token_ttl_days: i64,
    /// Exact origins allowed to call the API from a browser.
    ///
    /// Empty by default: native apps are not subject to CORS, so the browser
    /// surface stays closed until someone opts an origin in. Never a wildcard —
    /// see docs/research-2026.md §5.
    pub cors_allowed_origins: Vec<String>,
    /// Caps `gyms` at one row (ADR-0023) — the tenancy engine itself (`gym_id`,
    /// RLS, `TenantScope`) is unchanged and multi-gym-capable either way.
    ///
    /// **Off by default.** This is a product policy for a specific deployment,
    /// not a property of the engine, and the verification suites deliberately
    /// create several gyms to prove tenant isolation (`scripts/verify-rls.sh`,
    /// `scripts/verify-capacities.sh`) — so local dev stays permissive and a
    /// real single-gym deployment (see `docker-compose.demo.yml`) opts in
    /// explicitly, the same shape as `APP_ENV` gating the JWT-secret check.
    pub single_gym_mode: bool,
    /// The Stripe seam (ADR-0010). Both or neither: a secret key with no
    /// webhook secret could create checkout sessions that never get marked
    /// paid, which is worse than the feature being off. `None` means a member
    /// sees "card payment isn't set up for this gym yet" rather than the
    /// server refusing to boot — this is an optional capability, not a
    /// misconfiguration the way a missing `JWT_SECRET` would be.
    pub stripe_secret_key: Option<String>,
    pub stripe_webhook_secret: Option<String>,
    /// Which gateway sits behind ADR-0010's seam (ADR-0028).
    ///
    /// `dummy` serves a card page from this API and takes no money; `stripe`
    /// uses the real thing. Unset means `dummy` in a debug build and *nothing*
    /// in a release build — a shipped binary must never quietly accept fake
    /// cards, and a shipped binary with no gateway configured says card payment
    /// is unavailable, which is the honest answer.
    pub payment_gateway: String,
    /// Where this API is reachable from a browser, for the checkout redirect.
    ///
    /// Has to be absolute and has to be right: it is handed to a browser, not
    /// fetched by us, so a wrong value produces a link that works only for the
    /// person who configured it. Defaults to the bound address, which is
    /// correct for local development and for the same-origin demo.
    pub public_base_url: String,
    /// Where a link in an email should send someone (ADR-0029).
    ///
    /// The APP, not the API: a person clicking a reset link wants a screen,
    /// not JSON. Defaults to `public_base_url` so a single-origin deployment
    /// — which the demo is, with nginx serving the web build and proxying
    /// `/api` — needs no extra setting.
    pub link_base_url: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let environment = std::env::var("APP_ENV").unwrap_or_else(|_| "development".to_owned());
        let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_owned());
        let port = optional_parsed("SERVER_PORT", 8080)?;
        let jwt_secret = required("JWT_SECRET")?;

        if environment != "development" {
            if jwt_secret == INSECURE_DEV_SECRET {
                bail!(
                    "JWT_SECRET is still the development placeholder — set a real secret \
                     (openssl rand -base64 48) before running outside development"
                );
            }
            if jwt_secret.len() < MIN_SECRET_LEN {
                bail!("JWT_SECRET must be at least {MIN_SECRET_LEN} characters");
            }
        }

        let database_url = required("DATABASE_URL")?;
        let app_database_url = std::env::var("APP_DATABASE_URL").unwrap_or_else(|_| {
            tracing::warn!(
                "APP_DATABASE_URL is not set — falling back to DATABASE_URL. If that is the \
                 table owner, ROW-LEVEL SECURITY IS INERT and tenant isolation rests on \
                 application code alone."
            );
            database_url.clone()
        });

        Ok(Self {
            database_url,
            app_database_url,
            database_max_connections: optional_parsed("DATABASE_MAX_CONNECTIONS", 10)?,
            host: host.clone(),
            port,
            jwt_secret,
            access_token_ttl_seconds: optional_parsed("ACCESS_TOKEN_TTL_SECONDS", 900)?,
            refresh_token_ttl_days: optional_parsed("REFRESH_TOKEN_TTL_DAYS", 30)?,
            cors_allowed_origins: parse_origins(
                std::env::var("CORS_ALLOWED_ORIGINS").ok().as_deref(),
            ),
            single_gym_mode: optional_parsed("SINGLE_GYM_MODE", false)?,
            stripe_secret_key: non_empty_env("STRIPE_SECRET_KEY"),
            stripe_webhook_secret: non_empty_env("STRIPE_WEBHOOK_SECRET"),
            payment_gateway: non_empty_env("PAYMENT_GATEWAY").unwrap_or_else(|| {
                if cfg!(debug_assertions) {
                    "dummy".to_owned()
                } else {
                    "none".to_owned()
                }
            }),
            link_base_url: non_empty_env("LINK_BASE_URL")
                .or_else(|| non_empty_env("PUBLIC_BASE_URL"))
                .unwrap_or_else(|| {
                    let reachable = if host == "0.0.0.0" {
                        "127.0.0.1"
                    } else {
                        &host
                    };
                    format!("http://{reachable}:{port}")
                }),
            public_base_url: non_empty_env("PUBLIC_BASE_URL").unwrap_or_else(|| {
                // 0.0.0.0 is a bind address, not somewhere a browser can go.
                let reachable = if host == "0.0.0.0" {
                    "127.0.0.1"
                } else {
                    &host
                };
                format!("http://{reachable}:{port}")
            }),
        })
    }

    #[must_use]
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Split a comma-separated origin list, ignoring blanks.
fn parse_origins(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|o| !o.is_empty())
        .map(str::to_owned)
        .collect()
}

fn required(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("required environment variable {key} is not set"))
}

/// `None` for unset OR blank — an `.env` line left as `STRIPE_SECRET_KEY=`
/// must mean "off", not "authenticate to Stripe with an empty string".
fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

fn optional_parsed<T>(key: &str, default: T) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Err(_) => Ok(default),
        Ok(raw) => raw
            .parse::<T>()
            .map_err(|e| anyhow::anyhow!("invalid value for {key}: {e}")),
    }
}

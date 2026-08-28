//! Liveness and readiness.
//!
//! `/health` answers "is the process up?" — it must not touch the database, or a
//! DB blip will make orchestrators kill otherwise-healthy instances.
//! `/ready` answers "can it serve traffic?" and *does* check dependencies.

use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;

use crate::state::AppState;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ReadinessResponse {
    pub status: &'static str,
    pub database: &'static str,
}

/// Liveness probe.
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses((status = 200, description = "Process is alive", body = HealthResponse))
)]
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Readiness probe — verifies the database is reachable.
#[utoipa::path(
    get,
    path = "/ready",
    tag = "health",
    responses(
        (status = 200, description = "Ready to serve traffic", body = ReadinessResponse),
        (status = 503, description = "A dependency is unavailable", body = ReadinessResponse),
    )
)]
pub async fn readiness(State(state): State<AppState>) -> (StatusCode, Json<ReadinessResponse>) {
    match sqlx_ping(&state).await {
        Ok(()) => (
            StatusCode::OK,
            Json(ReadinessResponse {
                status: "ready",
                database: "up",
            }),
        ),
        Err(err) => {
            tracing::warn!(error = %err, "readiness check failed");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ReadinessResponse {
                    status: "not_ready",
                    database: "down",
                }),
            )
        }
    }
}

async fn sqlx_ping(state: &AppState) -> Result<(), sqlx::Error> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .map(|_| ())
}

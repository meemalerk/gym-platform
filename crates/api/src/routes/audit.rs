//! Audit trail for a gym.

use axum::{Json, extract::Query, extract::State};
use gym_application::ApplicationError;
use serde::{Deserialize, Serialize};

use crate::{error::ApiError, extract::TenantScope, state::AppState};

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct AuditQuery {
    /// Maximum entries to return (1–200, default 50).
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct AuditEntryResponse {
    pub id: uuid::Uuid,
    pub action: String,
    pub entity_type: String,
    pub entity_id: Option<uuid::Uuid>,
    pub actor_name: Option<String>,
    pub metadata: serde_json::Value,
    pub occurred_at: String,
}

/// Recent activity in this gym, newest first.
///
/// Restricted to people who manage the gym: an audit trail names who did what,
/// which is not something every member should be able to read.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/audit",
    tag = "audit",
    security(("bearer" = [])),
    params(
        ("gym_id" = uuid::Uuid, Path, description = "Gym id"),
        AuditQuery,
    ),
    responses(
        (status = 200, description = "Recent activity", body = Vec<AuditEntryResponse>),
        (status = 403, description = "Not permitted to read the audit trail"),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn list(
    State(state): State<AppState>,
    tenant: TenantScope,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditEntryResponse>>, ApiError> {
    if !tenant.capabilities.can_manage_gym() {
        return Err(ApplicationError::Forbidden.into());
    }

    let entries = state
        .audit
        .recent(&tenant, query.limit.unwrap_or(50))
        .await?;

    Ok(Json(
        entries
            .into_iter()
            .map(|e| AuditEntryResponse {
                id: e.id,
                action: e.action,
                entity_type: e.entity_type,
                entity_id: e.entity_id,
                actor_name: e.actor_name,
                metadata: e.metadata,
                occurred_at: e.occurred_at.to_rfc3339(),
            })
            .collect(),
    ))
}

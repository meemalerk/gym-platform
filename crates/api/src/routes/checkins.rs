//! Gym entry endpoints: a member's QR pass, and staff scanning it at the door.

use axum::{Json, extract::State, http::StatusCode};
use chrono::{DateTime, Utc};
use gym_domain::checkin::CheckIn;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, extract::TenantScope, state::AppState};

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct EntryPassResponse {
    /// Hand this to a QR renderer as-is — it is opaque to the client by
    /// design; only the server ever decodes it.
    pub token: String,
    pub expires_in_seconds: i64,
}

/// A short-lived pass for the caller to show at the door. Anyone signed in
/// and in this gym gets one — staff walk through the same door as everyone
/// else.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/checkins/my-pass",
    tag = "checkins",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    responses((status = 201, description = "Pass issued", body = EntryPassResponse))
)]
pub async fn my_pass(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<(StatusCode, Json<EntryPassResponse>), ApiError> {
    let pass = state.checkins.my_pass(&tenant).await?;
    Ok((
        StatusCode::CREATED,
        Json(EntryPassResponse {
            token: pass.token,
            expires_in_seconds: pass.expires_in_seconds,
        }),
    ))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct ScanRequest {
    /// Whatever the QR code decoded to — passed through unparsed; the server
    /// is the only thing that knows how to read it.
    pub token: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ScanResponse {
    pub allowed: bool,
    pub member_name: String,
    /// One line to show at the door — "Your Coaching membership" when
    /// allowed, "No active plan covers gym access." when not.
    pub reason: String,
    pub scanned_at: DateTime<Utc>,
}

fn scan_response(checkin: CheckIn, member_name: String) -> ScanResponse {
    ScanResponse {
        allowed: checkin.allowed,
        member_name,
        reason: checkin.reason,
        scanned_at: checkin.scanned_at,
    }
}

/// Staff scans a pass at the door.
///
/// Always `200`, allowed or not — a denial is the door's normal answer to a
/// lapsed membership, not a server error, and both outcomes are recorded the
/// same way (`gym_domain::checkin`'s module doc explains why).
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/checkins/scan",
    tag = "checkins",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    request_body = ScanRequest,
    responses(
        (status = 200, description = "Scanned — check `allowed`", body = ScanResponse),
        (status = 400, description = "The code is not valid or has expired"),
        (status = 403, description = "Front-desk capacity required (trainer and above)"),
    )
)]
pub async fn scan(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<ScanRequest>,
) -> Result<Json<ScanResponse>, ApiError> {
    let checkin = state.checkins.scan(&tenant, &body.token).await?;
    let member = state
        .users
        .find_by_id(checkin.member_id)
        .await?
        .map(|u| u.display_name)
        .unwrap_or_else(|| "Member".to_owned());

    Ok(Json(scan_response(checkin, member)))
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CheckInResponse {
    pub id: Uuid,
    pub member_id: Uuid,
    pub member_name: String,
    pub allowed: bool,
    pub reason: String,
    pub scanned_at: DateTime<Utc>,
}

/// The door's recent history — staff-only, newest first.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/checkins",
    tag = "checkins",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    responses((status = 200, description = "Recent check-ins", body = Vec<CheckInResponse>))
)]
pub async fn recent(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<Vec<CheckInResponse>>, ApiError> {
    let views = state.checkins.recent(&tenant).await?;
    Ok(Json(
        views
            .into_iter()
            .map(|v| CheckInResponse {
                id: v.checkin.id.into_uuid(),
                member_id: v.checkin.member_id.into_uuid(),
                member_name: v.member_name,
                allowed: v.checkin.allowed,
                reason: v.checkin.reason,
                scanned_at: v.checkin.scanned_at,
            })
            .collect(),
    ))
}

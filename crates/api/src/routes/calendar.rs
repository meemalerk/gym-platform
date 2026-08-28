//! The operating calendar (ADR-0015).
//!
//! Reading is open to anyone in the gym: "are you open on Friday?" is not a
//! privileged question, and a member who cannot see the hours cannot plan.
//! Writing the gym's hours is `can_manage_gym` — when the building is open is
//! a business fact, not a coaching one. Availability is the exception: a
//! trainer sets their own.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{Days, NaiveDate, NaiveTime};
use gym_domain::{
    UserId,
    calendar::{CalendarOverride, TimeSpan},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, extract::TenantScope, state::AppState};

/// Default window when a caller names neither end: a fortnight, which is what
/// "what's on?" means in practice.
const DEFAULT_WINDOW_DAYS: u64 = 13;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SpanRequest {
    /// 0 = Sunday, matching the database and JavaScript's `getDay()`.
    pub weekday: u8,
    #[schema(value_type = String, example = "06:00")]
    pub opens_at: NaiveTime,
    #[schema(value_type = String, example = "22:00")]
    pub closes_at: NaiveTime,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetHoursRequest {
    /// The complete weekly pattern. Sending it whole is deliberate — these are
    /// edited as one thing ("our hours"), and a partial update would need a
    /// merge rule the calendar spends its whole design avoiding.
    pub spans: Vec<SpanRequest>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct SetOverrideRequest {
    pub date: NaiveDate,
    /// `true` closes the day entirely; `false` requires hours.
    pub is_closed: bool,
    #[schema(value_type = Option<String>)]
    pub opens_at: Option<NaiveTime>,
    #[schema(value_type = Option<String>)]
    pub closes_at: Option<NaiveTime>,
    #[schema(example = "Public holiday")]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
pub struct WindowQuery {
    /// Defaults to today.
    pub from: Option<NaiveDate>,
    /// Defaults to a fortnight after `from`.
    pub to: Option<NaiveDate>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct SpanResponse {
    #[schema(value_type = String)]
    pub opens_at: NaiveTime,
    #[schema(value_type = String)]
    pub closes_at: NaiveTime,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct WeeklyHoursResponse {
    pub weekday: u8,
    #[schema(value_type = String)]
    pub opens_at: NaiveTime,
    #[schema(value_type = String)]
    pub closes_at: NaiveTime,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct OpeningDayResponse {
    pub date: NaiveDate,
    pub spans: Vec<SpanResponse>,
    /// Open at some point that day. Note this is not "open right now" — a gym
    /// with split hours is open today while being shut at noon.
    pub is_open: bool,
    /// True only when an override closed it. An unconfigured weekday is also
    /// shut, and the two are different facts — one is a decision, the other is
    /// a gap in the setup.
    pub closed_by_override: bool,
    #[schema(nullable)]
    pub reason: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CalendarResponse {
    /// IANA name. The times above are wall-clock in this zone — a client
    /// turning them into instants needs it, and guessing would be wrong twice
    /// a year.
    pub timezone: String,
    pub days: Vec<OpeningDayResponse>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BookableDayResponse {
    pub date: NaiveDate,
    pub spans: Vec<SpanResponse>,
}

fn to_span(span: TimeSpan) -> SpanResponse {
    SpanResponse {
        opens_at: span.opens_at,
        closes_at: span.closes_at,
    }
}

/// Resolve the calendar over a window.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/calendar",
    tag = "calendar",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id"), WindowQuery),
    responses(
        (status = 200, description = "Resolved days", body = CalendarResponse),
        (status = 400, description = "Window inverted or too long"),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn calendar(
    State(state): State<AppState>,
    tenant: TenantScope,
    Query(window): Query<WindowQuery>,
) -> Result<Json<CalendarResponse>, ApiError> {
    let (from, to) = resolve_window(&window);

    let resolved = state.calendar.resolve(&tenant, from, to).await?;

    Ok(Json(CalendarResponse {
        timezone: resolved.timezone,
        days: resolved
            .days
            .into_iter()
            .map(|day| OpeningDayResponse {
                date: day.date,
                is_open: day.opens_at_all(),
                closed_by_override: day.closed_by_override,
                reason: day.reason,
                spans: day.spans.into_iter().map(to_span).collect(),
            })
            .collect(),
    }))
}

/// The weekly pattern, unresolved — what an editor loads.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/calendar/hours",
    tag = "calendar",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    responses(
        (status = 200, description = "Weekly pattern", body = Vec<WeeklyHoursResponse>),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn hours(
    State(state): State<AppState>,
    tenant: TenantScope,
) -> Result<Json<Vec<WeeklyHoursResponse>>, ApiError> {
    let hours = state.calendar.opening_hours(&tenant).await?;
    Ok(Json(
        hours
            .into_iter()
            .map(|h| WeeklyHoursResponse {
                weekday: h.weekday,
                opens_at: h.span.opens_at,
                closes_at: h.span.closes_at,
            })
            .collect(),
    ))
}

/// Replace the weekly pattern. Owners and admins.
#[utoipa::path(
    put,
    path = "/api/v1/gyms/{gym_id}/calendar/hours",
    tag = "calendar",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    request_body = SetHoursRequest,
    responses(
        (status = 200, description = "Replaced", body = Vec<WeeklyHoursResponse>),
        (status = 400, description = "A span closes before it opens, or names no weekday"),
        (status = 403, description = "Only owners and admins set opening hours"),
    )
)]
pub async fn set_hours(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<SetHoursRequest>,
) -> Result<Json<Vec<WeeklyHoursResponse>>, ApiError> {
    let spans = body
        .spans
        .into_iter()
        .map(|s| Ok((s.weekday, TimeSpan::new(s.opens_at, s.closes_at)?)))
        .collect::<Result<Vec<_>, gym_domain::DomainError>>()
        .map_err(gym_application::ApplicationError::Domain)?;

    let saved = state.calendar.set_opening_hours(&tenant, spans).await?;

    Ok(Json(
        saved
            .into_iter()
            .map(|h| WeeklyHoursResponse {
                weekday: h.weekday,
                opens_at: h.span.opens_at,
                closes_at: h.span.closes_at,
            })
            .collect(),
    ))
}

/// Close a date, or give it special hours. Setting the same date twice
/// replaces it — correcting a date you already set is normal, not an error.
#[utoipa::path(
    put,
    path = "/api/v1/gyms/{gym_id}/calendar/overrides",
    tag = "calendar",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    request_body = SetOverrideRequest,
    responses(
        (status = 204, description = "Set"),
        (status = 400, description = "Closed with hours, or open without them"),
        (status = 403, description = "Only owners and admins change the calendar"),
    )
)]
pub async fn set_override(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<SetOverrideRequest>,
) -> Result<StatusCode, ApiError> {
    let entry = if body.is_closed {
        CalendarOverride::closed(tenant.gym_id, body.date, body.reason.as_deref())
    } else {
        let (Some(opens_at), Some(closes_at)) = (body.opens_at, body.closes_at) else {
            return Err(gym_application::ApplicationError::Domain(
                gym_domain::DomainError::Invalid(
                    "special hours need both an opening and a closing time".to_owned(),
                ),
            )
            .into());
        };
        TimeSpan::new(opens_at, closes_at).and_then(|span| {
            CalendarOverride::special_hours(tenant.gym_id, body.date, span, body.reason.as_deref())
        })
    }
    .map_err(gym_application::ApplicationError::Domain)?;

    state.calendar.set_override(&tenant, entry).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Drop an override so the weekly pattern applies again.
#[utoipa::path(
    delete,
    path = "/api/v1/gyms/{gym_id}/calendar/overrides/{date}",
    tag = "calendar",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("date" = String, Path, description = "The date, YYYY-MM-DD"),
    ),
    responses(
        (status = 204, description = "Cleared"),
        (status = 403, description = "Only owners and admins change the calendar"),
        (status = 404, description = "That date had no override"),
    )
)]
pub async fn clear_override(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, date)): Path<(Uuid, NaiveDate)>,
) -> Result<StatusCode, ApiError> {
    state.calendar.clear_override(&tenant, date).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// One trainer's weekly availability.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/trainers/{trainer_id}/availability",
    tag = "calendar",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("trainer_id" = Uuid, Path, description = "Trainer id"),
    ),
    responses(
        (status = 200, description = "Weekly availability", body = Vec<WeeklyHoursResponse>),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn availability(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, trainer)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<WeeklyHoursResponse>>, ApiError> {
    let hours = state
        .calendar
        .availability(&tenant, UserId::from(trainer))
        .await?;

    Ok(Json(
        hours
            .into_iter()
            .map(|h| WeeklyHoursResponse {
                weekday: h.weekday,
                opens_at: h.span.opens_at,
                closes_at: h.span.closes_at,
            })
            .collect(),
    ))
}

/// Set availability — your own, or a manager setting somebody's.
#[utoipa::path(
    put,
    path = "/api/v1/gyms/{gym_id}/trainers/{trainer_id}/availability",
    tag = "calendar",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("trainer_id" = Uuid, Path, description = "Trainer id"),
    ),
    request_body = SetHoursRequest,
    responses(
        (status = 200, description = "Replaced", body = Vec<WeeklyHoursResponse>),
        (status = 400, description = "A span closes before it opens"),
        (status = 403, description = "Not yours to set"),
        (status = 404, description = "That person does not coach here"),
    )
)]
pub async fn set_availability(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, trainer)): Path<(Uuid, Uuid)>,
    Json(body): Json<SetHoursRequest>,
) -> Result<Json<Vec<WeeklyHoursResponse>>, ApiError> {
    let spans = body
        .spans
        .into_iter()
        .map(|s| Ok((s.weekday, TimeSpan::new(s.opens_at, s.closes_at)?)))
        .collect::<Result<Vec<_>, gym_domain::DomainError>>()
        .map_err(gym_application::ApplicationError::Domain)?;

    let saved = state
        .calendar
        .set_availability(&tenant, UserId::from(trainer), spans)
        .await?;

    Ok(Json(
        saved
            .into_iter()
            .map(|h| WeeklyHoursResponse {
                weekday: h.weekday,
                opens_at: h.span.opens_at,
                closes_at: h.span.closes_at,
            })
            .collect(),
    ))
}

/// When a trainer can actually be booked: their availability, clipped to the
/// gym being open.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/trainers/{trainer_id}/bookable",
    tag = "calendar",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("trainer_id" = Uuid, Path, description = "Trainer id"),
        WindowQuery,
    ),
    responses(
        (status = 200, description = "Bookable spans per day", body = Vec<BookableDayResponse>),
        (status = 400, description = "Window inverted or too long"),
        (status = 404, description = "Gym not found or caller is not a member"),
    )
)]
pub async fn bookable(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, trainer)): Path<(Uuid, Uuid)>,
    Query(window): Query<WindowQuery>,
) -> Result<Json<Vec<BookableDayResponse>>, ApiError> {
    let (from, to) = resolve_window(&window);

    let days = state
        .calendar
        .bookable(&tenant, UserId::from(trainer), from, to)
        .await?;

    Ok(Json(
        days.into_iter()
            .map(|(date, spans)| BookableDayResponse {
                date,
                spans: spans.into_iter().map(to_span).collect(),
            })
            .collect(),
    ))
}

/// Fill in whichever end of the window the caller left out.
///
/// "Today" here is the SERVER's today, in UTC, which is the same limitation
/// the session filters carry: the gym's own day needs its timezone applied,
/// and the honest fix is for a client that knows the gym's zone to send
/// explicit dates. It does not guess.
fn resolve_window(window: &WindowQuery) -> (NaiveDate, NaiveDate) {
    let from = window
        .from
        .unwrap_or_else(|| chrono::Utc::now().date_naive());
    let to = window.to.unwrap_or_else(|| {
        from.checked_add_days(Days::new(DEFAULT_WINDOW_DAYS))
            .unwrap_or(from)
    });
    (from, to)
}

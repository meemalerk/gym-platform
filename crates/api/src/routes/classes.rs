//! Group-class endpoints: the timetable, booking a place, and the roster.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use chrono::{NaiveDate, NaiveTime};
use gym_application::ports::ClassOnDate;
use gym_domain::{ClassBookingId, GymClassId, gym_class::weekday_name};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, extract::TenantScope, state::AppState};

/// One row of the timetable: a class on a specific date, with occupancy.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ClassOccurrenceResponse {
    pub class_id: Uuid,
    pub name: String,
    #[schema(nullable)]
    pub description: Option<String>,
    pub instructor_id: Uuid,
    pub instructor_name: String,
    /// 0 = Sunday, matching the gym's opening hours and JavaScript `getDay()`.
    pub weekday: u8,
    /// Spelled out so a client never has to map the number itself — one
    /// spelling of the convention, on the server.
    pub weekday_name: String,
    #[schema(value_type = String, example = "18:00:00")]
    pub starts_at: NaiveTime,
    pub duration_minutes: u16,
    #[schema(value_type = String, example = "2026-08-31")]
    pub on_date: NaiveDate,
    pub capacity: u32,
    pub booked: u32,
    /// `capacity - booked`, never negative. Computed here so three clients do
    /// not each get the subtraction subtly wrong.
    pub places_left: u32,
    pub is_full: bool,
    /// Whether the CALLER holds a place — what the Book/Cancel button keys on.
    pub booked_by_me: bool,
    /// The caller's own booking id, when they hold one. Cancel needs it, and
    /// without it a client can render "Booked" with no way to undo it.
    #[schema(nullable)]
    pub my_booking_id: Option<Uuid>,
}

fn occurrence_response(row: ClassOnDate) -> ClassOccurrenceResponse {
    let capacity = row.class.capacity;
    let booked = row.booked;
    ClassOccurrenceResponse {
        class_id: row.class.id.into_uuid(),
        name: row.class.name,
        description: row.class.description,
        instructor_id: row.class.instructor_id.into_uuid(),
        instructor_name: row.instructor_name,
        weekday: row.class.weekday,
        weekday_name: weekday_name(row.class.weekday).to_owned(),
        starts_at: row.class.starts_at,
        duration_minutes: row.class.duration_minutes,
        on_date: row.on_date,
        capacity,
        booked,
        places_left: capacity.saturating_sub(booked),
        is_full: booked >= capacity,
        booked_by_me: row.booked_by_me,
        my_booking_id: row.my_booking_id.map(|id| id.into_uuid()),
    }
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct TimetableQuery {
    /// Defaults to today.
    pub from: Option<NaiveDate>,
    /// Defaults to a week after `from` — "classes this week", which is what
    /// every dashboard asks for.
    pub to: Option<NaiveDate>,
}

/// What is on, and whether the caller is in it.
///
/// Open to everyone in the gym: a member books from it, a trainer finds their
/// own classes in it, a manager runs the place from it.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/classes",
    tag = "classes",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id"), TimetableQuery),
    responses((status = 200, description = "The timetable", body = [ClassOccurrenceResponse]))
)]
pub async fn timetable(
    State(state): State<AppState>,
    tenant: TenantScope,
    Query(q): Query<TimetableQuery>,
) -> Result<Json<Vec<ClassOccurrenceResponse>>, ApiError> {
    let from = q.from.unwrap_or_else(|| chrono::Utc::now().date_naive());
    let to = q.to.unwrap_or(from + chrono::Duration::days(6));

    let rows = state.classes.timetable(&tenant, from, to).await?;
    Ok(Json(rows.into_iter().map(occurrence_response).collect()))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateClassRequest {
    pub name: String,
    pub instructor_id: Uuid,
    /// 0 = Sunday.
    pub weekday: u8,
    #[schema(value_type = String, example = "18:00:00")]
    pub starts_at: NaiveTime,
    pub duration_minutes: u16,
    pub capacity: u32,
    #[schema(nullable)]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ClassResponse {
    pub id: Uuid,
    pub name: String,
    pub instructor_id: Uuid,
    pub weekday: u8,
    pub weekday_name: String,
    #[schema(value_type = String, example = "18:00:00")]
    pub starts_at: NaiveTime,
    pub duration_minutes: u16,
    pub capacity: u32,
    #[schema(nullable)]
    pub description: Option<String>,
    pub is_live: bool,
}

fn class_response(class: gym_domain::gym_class::GymClass) -> ClassResponse {
    ClassResponse {
        id: class.id.into_uuid(),
        name: class.name,
        instructor_id: class.instructor_id.into_uuid(),
        weekday: class.weekday,
        weekday_name: weekday_name(class.weekday).to_owned(),
        starts_at: class.starts_at,
        duration_minutes: class.duration_minutes,
        capacity: class.capacity,
        description: class.description,
        is_live: class.archived_at.is_none(),
    }
}

/// Put a class on the timetable. Owners and admins.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/classes",
    tag = "classes",
    security(("bearer" = [])),
    params(("gym_id" = Uuid, Path, description = "Gym id")),
    request_body = CreateClassRequest,
    responses(
        (status = 201, description = "Added", body = ClassResponse),
        (status = 403, description = "Managers only"),
        (status = 409, description = "That name already runs at that time"),
    )
)]
pub async fn create(
    State(state): State<AppState>,
    tenant: TenantScope,
    Json(body): Json<CreateClassRequest>,
) -> Result<(StatusCode, Json<ClassResponse>), ApiError> {
    let class = state
        .classes
        .create(
            &tenant,
            &body.name,
            gym_domain::UserId::from(body.instructor_id),
            body.weekday,
            body.starts_at,
            body.duration_minutes,
            body.capacity,
            body.description.as_deref(),
        )
        .await?;

    Ok((StatusCode::CREATED, Json(class_response(class))))
}

/// Take a class off the timetable. Owners and admins.
///
/// Archived, not deleted — past bookings still reference it.
#[utoipa::path(
    delete,
    path = "/api/v1/gyms/{gym_id}/classes/{class_id}",
    tag = "classes",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("class_id" = Uuid, Path, description = "Class id"),
    ),
    responses(
        (status = 200, description = "Off the timetable", body = ClassResponse),
        (status = 403, description = "Managers only"),
        (status = 404, description = "No such class"),
    )
)]
pub async fn archive(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, class_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ClassResponse>, ApiError> {
    let class = state
        .classes
        .archive(&tenant, GymClassId::from(class_id))
        .await?;
    Ok(Json(class_response(class)))
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct BookRequest {
    /// Minted on the device (ADR-0008), so a retry replays the same booking
    /// instead of taking a second place.
    pub id: Uuid,
    /// Which sitting. Must fall on the class's weekday.
    #[schema(value_type = String, example = "2026-08-31")]
    pub on_date: NaiveDate,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct BookingResponse {
    pub id: Uuid,
    pub class_id: Uuid,
    pub member_id: Uuid,
    #[schema(value_type = String, example = "2026-08-31")]
    pub on_date: NaiveDate,
    pub is_live: bool,
}

fn booking_response(b: gym_domain::gym_class::ClassBooking) -> BookingResponse {
    BookingResponse {
        id: b.id.into_uuid(),
        class_id: b.class_id.into_uuid(),
        member_id: b.member_id.into_uuid(),
        on_date: b.on_date,
        is_live: b.cancelled_at.is_none(),
    }
}

/// Take a place in one sitting, for yourself.
#[utoipa::path(
    post,
    path = "/api/v1/gyms/{gym_id}/classes/{class_id}/bookings",
    tag = "classes",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("class_id" = Uuid, Path, description = "Class id"),
    ),
    request_body = BookRequest,
    responses(
        (status = 201, description = "Booked", body = BookingResponse),
        (status = 400, description = "That date is not the class's weekday"),
        (status = 403, description = "No active plan covers gym access"),
        (status = 404, description = "No such class"),
        (status = 409, description = "Full, already started, or already booked"),
    )
)]
pub async fn book(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, class_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<BookRequest>,
) -> Result<(StatusCode, Json<BookingResponse>), ApiError> {
    let booking = state
        .classes
        .book(
            &tenant,
            ClassBookingId::from(body.id),
            GymClassId::from(class_id),
            body.on_date,
        )
        .await?;

    Ok((StatusCode::CREATED, Json(booking_response(booking))))
}

/// Give your place back. Allowed until the class starts.
#[utoipa::path(
    delete,
    path = "/api/v1/gyms/{gym_id}/class-bookings/{booking_id}",
    tag = "classes",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("booking_id" = Uuid, Path, description = "Booking id"),
    ),
    responses(
        (status = 200, description = "Place released", body = BookingResponse),
        (status = 404, description = "Not your booking, or no such booking"),
        (status = 409, description = "Already started, or already cancelled"),
    )
)]
pub async fn cancel_booking(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, booking_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<BookingResponse>, ApiError> {
    let booking = state
        .classes
        .cancel_booking(&tenant, ClassBookingId::from(booking_id))
        .await?;
    Ok(Json(booking_response(booking)))
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct RosterQuery {
    #[param(value_type = String, example = "2026-08-31")]
    pub on_date: NaiveDate,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct RosterEntry {
    pub member_id: Uuid,
    pub member_name: String,
}

/// Who is booked into one sitting. The class's own instructor, or a manager.
#[utoipa::path(
    get,
    path = "/api/v1/gyms/{gym_id}/classes/{class_id}/roster",
    tag = "classes",
    security(("bearer" = [])),
    params(
        ("gym_id" = Uuid, Path, description = "Gym id"),
        ("class_id" = Uuid, Path, description = "Class id"),
        RosterQuery,
    ),
    responses(
        (status = 200, description = "The roster", body = [RosterEntry]),
        (status = 403, description = "Not this class's instructor"),
        (status = 404, description = "No such class"),
    )
)]
pub async fn roster(
    State(state): State<AppState>,
    tenant: TenantScope,
    Path((_gym, class_id)): Path<(Uuid, Uuid)>,
    Query(q): Query<RosterQuery>,
) -> Result<Json<Vec<RosterEntry>>, ApiError> {
    let rows = state
        .classes
        .roster(&tenant, GymClassId::from(class_id), q.on_date)
        .await?;

    Ok(Json(
        rows.into_iter()
            .map(|(id, member_name)| RosterEntry {
                member_id: id.into_uuid(),
                member_name,
            })
            .collect(),
    ))
}

//! Pure domain types and invariants. No I/O, no HTTP, no SQL.
//!
//! Principle (see docs/domain-model.md): make invalid states unrepresentable.
//! Prefer enums over bags of nullable fields.

pub mod assignment;
pub mod billing;
pub mod calendar;
pub mod checkin;
pub mod coaching;
pub mod coaching_request;
pub mod entitlement;
pub mod execution;
pub mod exercise;
pub mod goal;
pub mod gym;
pub mod gym_class;
pub mod ids;
pub mod measurement;
pub mod prescription;
pub mod profile;
pub mod program;
pub mod tenancy;
pub mod user;
pub mod workout;

pub use assignment::{AssignmentError, AssignmentStatus, ProgramAssignment};
pub use billing::{
    BillingInterval, Invoice, InvoiceStatus, MemberSubscription, MembershipPlan, Payment,
    PaymentProvider, SubscriptionStatus, format_money,
};
pub use calendar::{
    CalendarOverride, OpeningDay, TimeSpan, WeeklyHours, bookable_spans, resolve_day,
};
pub use checkin::CheckIn;
pub use coaching::{
    CoachRelationship, CoachingError, RelationshipStatus, may_coach_athlete, may_view_athlete,
};
pub use coaching_request::{CoachingRequest, RequestError, RequestStatus};
pub use entitlement::{EntitlementSet, Feature, Source as EntitlementSource};
pub use execution::{ExecutionError, PerformedSet, PerformedValue, SessionStatus, WorkoutSession};
pub use goal::{Goal, GoalMetric, GoalStatus};
pub use gym_class::{ClassBooking, ClassError, ClassOccurrence, GymClass};
pub use ids::{
    AssignmentId, CalendarEntryId, CheckInId, ClassBookingId, CoachRelationshipId,
    CoachingRequestId, ExerciseId, GoalId, GymClassId, GymId, InvoiceId, MembershipId,
    MembershipPlanId, PaymentId, PerformedSetId, ProgramId, ProgramVersionId, ProgramWeekId,
    SessionId, SubscriptionId, TemplateExerciseId, UserId, WorkoutSessionId, WorkoutTemplateId,
};
pub use measurement::{BodyMeasurement, MeasurementEntry};
pub use prescription::{ExercisePrescription, PaceRange, RepRange};
pub use profile::{AthleteProfile, TrainerProfile};
pub use program::{
    ApprovalPolicy, LifecycleError, Program, ProgramFocus, ProgramVersion, VersionStatus,
};
pub use tenancy::{Capabilities, Capacity, TenantContext};
pub use workout::{ProgramWeek, TemplateExercise, WorkoutTemplate};

/// Errors raised by domain invariants — "is this action valid?"
///
/// Distinct from authorization ("may this actor attempt it?") — see
/// docs/authorization-model.md.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("{field} must not be empty")]
    Empty { field: &'static str },

    #[error("{field} must be at most {max} characters (got {actual})")]
    TooLong {
        field: &'static str,
        max: usize,
        actual: usize,
    },

    #[error("invalid email address")]
    InvalidEmail,

    #[error("{0}")]
    Invalid(String),
}

/// Trim and validate a required, length-bounded name-like field.
pub fn validated_name(field: &'static str, raw: &str, max: usize) -> Result<String, DomainError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(DomainError::Empty { field });
    }
    let len = trimmed.chars().count();
    if len > max {
        return Err(DomainError::TooLong {
            field,
            max,
            actual: len,
        });
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_blank_name() {
        assert_eq!(
            validated_name("name", "   ", 10),
            Err(DomainError::Empty { field: "name" })
        );
    }

    #[test]
    fn trims_and_accepts_valid_name() {
        assert_eq!(validated_name("name", "  Squat  ", 10).unwrap(), "Squat");
    }

    #[test]
    fn rejects_overlong_name_by_chars_not_bytes() {
        // 4 multi-byte chars must count as 4, not 8+ bytes.
        assert!(validated_name("name", "éééé", 4).is_ok());
        assert!(matches!(
            validated_name("name", "ééééé", 4),
            Err(DomainError::TooLong { actual: 5, .. })
        ));
    }
}

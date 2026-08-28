//! Typed identifiers. Newtypes stop us passing a `UserId` where a `GymId` belongs.
//!
//! UUIDv7 everywhere: time-ordered (index-friendly) and generatable on-device,
//! which the offline member app needs (see ADR-0008).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! typed_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord,
            Serialize, Deserialize, utoipa::ToSchema,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// Generate a new time-ordered identifier.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            #[must_use]
            pub const fn into_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl From<Uuid> for $name {
            fn from(id: Uuid) -> Self {
                Self(id)
            }
        }

        impl From<$name> for Uuid {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

typed_id!(
    /// A gym organisation — the tenant boundary.
    GymId
);
typed_id!(UserId);
typed_id!(MembershipId);
typed_id!(ExerciseId);
typed_id!(SessionId);

typed_id!(
    /// A programme — the stable identity a member is coached on. Its *content*
    /// lives in versions, never on the programme itself (ADR-0006).
    ProgramId
);
typed_id!(
    /// One immutable-once-published snapshot of a programme's content.
    ProgramVersionId
);
typed_id!(
    /// A coach–athlete pairing within one gym.
    CoachRelationshipId
);
typed_id!(
    /// A member asking a specific coach to take them on (ADR-0025).
    CoachingRequestId
);
typed_id!(
    /// One row of the operating calendar — a weekly opening span, a dated
    /// override, or a trainer's availability (ADR-0015). One id type for all
    /// three because they are the same shape, which is the point.
    CalendarEntryId
);
typed_id!(
    /// One athlete's assignment to one specific programme version.
    AssignmentId
);
typed_id!(
    /// One visit to the gym. Distinct from `SessionId`, which is an auth
    /// refresh-token session — an unfortunate but pre-existing name.
    WorkoutSessionId
);
typed_id!(PerformedSetId);
typed_id!(
    /// A measurable target for one athlete in one gym.
    GoalId
);
typed_id!(
    /// What a gym sells: a membership or coaching plan.
    MembershipPlanId
);
typed_id!(
    /// One member on one plan, at the price agreed when they joined it.
    SubscriptionId
);
typed_id!(InvoiceId);
typed_id!(PaymentId);
typed_id!(
    /// One scan at the door — allowed or not, the record is the same shape
    /// either way. A denial is as worth keeping as an admission: "who tried
    /// and couldn't" is real operational data.
    CheckInId
);
typed_id!(ProgramWeekId);
typed_id!(WorkoutTemplateId);
typed_id!(TemplateExerciseId);
typed_id!(
    /// A recurring weekly class slot — "Zumba, Mondays at 18:00". Not one
    /// sitting of it: an occurrence is this id plus a date, derived at read
    /// time rather than stored.
    GymClassId
);
typed_id!(
    /// One member's place in one occurrence of a class.
    ClassBookingId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_time_ordered() {
        let a = ExerciseId::new();
        let b = ExerciseId::new();
        assert!(a < b, "UUIDv7 ids should sort by creation time");
    }

    #[test]
    fn distinct_ids_are_distinct_types() {
        // Compile-time guarantee; this test documents the intent.
        let gym = GymId::new();
        let uuid: Uuid = gym.into_uuid();
        assert_eq!(GymId::from(uuid), gym);
    }
}

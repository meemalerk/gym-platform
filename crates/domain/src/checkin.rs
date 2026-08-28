//! Gym entry: a member's short-lived QR pass, scanned by staff at the door.
//!
//! Deliberately not a lifecycle like a workout session or a programme
//! version — a check-in is over the instant it happens. What is worth
//! keeping is the RECORD of the scan, allowed or not, in the same shape
//! either way: "who tried to get in and couldn't" is real operational data,
//! not noise to discard (the same instinct as ADR-0022's seed rules —
//! a demo, or a door log, that only shows the happy path is a to-do list).
//!
//! The pass itself is never stored. It is a short-lived signed token
//! (`gym_infrastructure::checkin_pass`) verified at the point of scanning —
//! matching ADR-0018's "compute, don't store" logic for anything whose truth
//! depends on the clock.

use chrono::{DateTime, Utc};

use crate::{CheckInId, DomainError, GymId, UserId};

/// One scan, and its outcome. `allowed` and `reason` are the OUTPUT of
/// `EntitlementSet::because`/absence at the moment of the scan — this struct
/// does not re-derive it later, because "why" can change (a plan lapses) and
/// the record must keep saying what was true when the door actually opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckIn {
    pub id: CheckInId,
    pub gym_id: GymId,
    pub member_id: UserId,
    /// Whoever held the scanner — a trainer or manager, never the member
    /// themselves (see `CheckInService::scan`'s authorization).
    pub scanned_by: UserId,
    pub allowed: bool,
    /// One line a screen can print verbatim — "Your Coaching membership" or
    /// "No active plan covers gym access."
    pub reason: String,
    pub scanned_at: DateTime<Utc>,
}

const MAX_REASON_LEN: usize = 200;

impl CheckIn {
    pub fn new(
        id: CheckInId,
        gym_id: GymId,
        member_id: UserId,
        scanned_by: UserId,
        allowed: bool,
        reason: &str,
        scanned_at: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        let reason = crate::validated_name("reason", reason, MAX_REASON_LEN)?;
        Ok(Self {
            id,
            gym_id,
            member_id,
            scanned_by,
            allowed,
            reason,
            scanned_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_an_admission() {
        let checkin = CheckIn::new(
            CheckInId::new(),
            GymId::new(),
            UserId::new(),
            UserId::new(),
            true,
            "Your Coaching membership",
            Utc::now(),
        )
        .unwrap();
        assert!(checkin.allowed);
    }

    #[test]
    fn records_a_denial_the_same_shape() {
        let checkin = CheckIn::new(
            CheckInId::new(),
            GymId::new(),
            UserId::new(),
            UserId::new(),
            false,
            "No active plan covers gym access.",
            Utc::now(),
        )
        .unwrap();
        assert!(!checkin.allowed);
    }

    #[test]
    fn rejects_a_blank_reason() {
        assert!(
            CheckIn::new(
                CheckInId::new(),
                GymId::new(),
                UserId::new(),
                UserId::new(),
                true,
                "   ",
                Utc::now(),
            )
            .is_err()
        );
    }
}

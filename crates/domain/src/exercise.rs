//! Exercise catalogue — the first tenant-scoped domain slice.
//!
//! Every exercise belongs to exactly one gym (`gym_id`), per ADR-0004.

use serde::{Deserialize, Serialize};

use crate::{
    DomainError,
    ids::{ExerciseId, GymId, UserId},
    validated_name,
};

/// Where a movement stands in the shared catalogue (ADR-0024).
///
/// The reason this exists is not tidiness. Progress is computed per
/// `exercise_id` (ADR-0018), so "DB Bench" and "Dumbbell Bench Press" as two
/// rows means one athlete's estimated-1RM history silently becomes two
/// half-charts, and no later edit can rejoin them — the performed sets point
/// at different ids for good. Curation is the only thing standing between an
/// open catalogue and that outcome.
///
/// A proposal is **usable immediately** by whoever raised it. Blocking a
/// trainer mid-programme until someone reviews a movement would trade a data
/// problem for a worse workflow problem; the point is to catch duplicates
/// before they spread, not to stop work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CatalogueStatus {
    /// Raised by a coach, usable by them, awaiting curation.
    Proposed,
    /// Part of the gym's vocabulary. The default for anything a catalogue
    /// manager creates directly — they are the curator, so there is nobody
    /// to review it past.
    Approved,
    /// No longer offered for new prescriptions. **Never deleted**: performed
    /// sets reference it, and history does not stop being true because the
    /// gym stopped programming the movement.
    Retired,
}

impl CatalogueStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Approved => "approved",
            Self::Retired => "retired",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "proposed" => Self::Proposed,
            "approved" => Self::Approved,
            "retired" => Self::Retired,
            _ => return None,
        })
    }

    /// May this movement be written into a NEW prescription?
    ///
    /// Retired ones may not. Existing prescriptions that already name one are
    /// untouched — a published version is immutable (ADR-0006), and retiring
    /// a movement is not a licence to rewrite what athletes were given.
    #[must_use]
    pub const fn is_prescribable(self) -> bool {
        matches!(self, Self::Proposed | Self::Approved)
    }
}

/// How an exercise is measured. Modelled as an enum so impossible combinations
/// (e.g. "reps AND distance AND duration all set") cannot be represented —
/// see docs/domain-model.md.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Modality {
    /// Sets and repetitions, e.g. back squat.
    Repetitions,
    /// Held or worked for time, e.g. plank.
    Duration,
    /// Covered distance, e.g. rowing.
    Distance,
}

impl Modality {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Repetitions => "repetitions",
            Self::Duration => "duration",
            Self::Distance => "distance",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "repetitions" => Self::Repetitions,
            "duration" => Self::Duration,
            "distance" => Self::Distance,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Exercise {
    pub id: ExerciseId,
    pub gym_id: GymId,
    pub name: String,
    pub modality: Modality,
    #[schema(nullable)]
    pub notes: Option<String>,
    pub status: CatalogueStatus,
    /// Who raised it. Kept for every exercise, not just proposals: a curator
    /// reviewing a duplicate needs to know who to talk to, and that stays true
    /// after the thing is approved.
    pub proposed_by: UserId,
}

impl Exercise {
    /// Create a new exercise within a gym, enforcing domain invariants.
    ///
    /// The status is decided by the caller's standing rather than passed in as
    /// data — `Exercise::new(.., status)` would let a route hand the domain a
    /// value the caller had no right to, which is exactly the shape of bug the
    /// prescription validator exists to prevent elsewhere.
    pub fn proposed_by(
        gym_id: GymId,
        name: &str,
        modality: Modality,
        notes: Option<&str>,
        author: UserId,
        may_curate: bool,
    ) -> Result<Self, DomainError> {
        let notes = match notes.map(str::trim) {
            None | Some("") => None,
            Some(text) => Some(validated_name("notes", text, 1000)?),
        };

        Ok(Self {
            id: ExerciseId::new(),
            gym_id,
            name: validated_name("name", name, 120)?,
            modality,
            notes,
            // A curator's own entry is already curated. Making them approve
            // their own proposal would be theatre, and the second-person rule
            // that makes sense for programmes does not transfer: a movement
            // name is not a commitment the gym has to honour.
            status: if may_curate {
                CatalogueStatus::Approved
            } else {
                CatalogueStatus::Proposed
            },
            proposed_by: author,
        })
    }

    /// Promote a proposal into the gym's vocabulary.
    pub fn approve(&mut self) -> Result<(), DomainError> {
        match self.status {
            CatalogueStatus::Proposed => {
                self.status = CatalogueStatus::Approved;
                Ok(())
            }
            CatalogueStatus::Approved => Err(DomainError::Invalid(
                "this movement is already in the catalogue".to_owned(),
            )),
            CatalogueStatus::Retired => Err(DomainError::Invalid(
                "retired movements are reinstated, not approved".to_owned(),
            )),
        }
    }

    /// Stop offering it for new prescriptions. Idempotent-ish: retiring an
    /// already-retired movement is a no-op the caller should not be punished
    /// for, but it is still worth refusing so a double-tap is visible.
    pub fn retire(&mut self) -> Result<(), DomainError> {
        if self.status == CatalogueStatus::Retired {
            return Err(DomainError::Invalid(
                "this movement is already retired".to_owned(),
            ));
        }
        self.status = CatalogueStatus::Retired;
        Ok(())
    }

    /// Bring a retired movement back. The gym changed its mind, which happens.
    pub fn reinstate(&mut self) -> Result<(), DomainError> {
        if self.status != CatalogueStatus::Retired {
            return Err(DomainError::Invalid(
                "this movement is not retired".to_owned(),
            ));
        }
        self.status = CatalogueStatus::Approved;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gym() -> GymId {
        GymId::new()
    }

    fn made_by_coach(name: &str) -> Exercise {
        Exercise::proposed_by(
            gym(),
            name,
            Modality::Repetitions,
            None,
            UserId::new(),
            false,
        )
        .unwrap()
    }

    #[test]
    fn creates_valid_exercise() {
        let ex = Exercise::proposed_by(
            gym(),
            "  Back Squat ",
            Modality::Repetitions,
            None,
            UserId::new(),
            true,
        )
        .unwrap();
        assert_eq!(ex.name, "Back Squat");
        assert_eq!(ex.modality, Modality::Repetitions);
        assert!(ex.notes.is_none());
    }

    #[test]
    fn blank_notes_normalise_to_none() {
        let ex = Exercise::proposed_by(
            gym(),
            "Plank",
            Modality::Duration,
            Some("   "),
            UserId::new(),
            true,
        )
        .unwrap();
        assert!(ex.notes.is_none());
    }

    #[test]
    fn rejects_empty_name() {
        assert_eq!(
            Exercise::proposed_by(gym(), "  ", Modality::Distance, None, UserId::new(), true)
                .map(|e| e.name),
            Err(DomainError::Empty { field: "name" })
        );
    }

    #[test]
    fn rejects_overlong_name() {
        let long = "x".repeat(121);
        assert!(matches!(
            Exercise::proposed_by(
                gym(),
                &long,
                Modality::Repetitions,
                None,
                UserId::new(),
                true
            ),
            Err(DomainError::TooLong { max: 120, .. })
        ));
    }

    #[test]
    fn modality_round_trips() {
        for m in [
            Modality::Repetitions,
            Modality::Duration,
            Modality::Distance,
        ] {
            assert_eq!(Modality::parse(m.as_str()), Some(m.clone()));
        }
        assert_eq!(Modality::parse("telekinesis"), None);
    }

    #[test]
    fn catalogue_status_round_trips() {
        for s in [
            CatalogueStatus::Proposed,
            CatalogueStatus::Approved,
            CatalogueStatus::Retired,
        ] {
            assert_eq!(CatalogueStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(CatalogueStatus::parse("pending"), None);
    }

    #[test]
    fn a_curators_own_entry_needs_no_review() {
        let ex = Exercise::proposed_by(
            gym(),
            "Deadlift",
            Modality::Repetitions,
            None,
            UserId::new(),
            true,
        )
        .unwrap();
        assert_eq!(ex.status, CatalogueStatus::Approved);
    }

    #[test]
    fn a_coachs_entry_starts_as_a_proposal() {
        assert_eq!(
            made_by_coach("Zercher Squat").status,
            CatalogueStatus::Proposed
        );
    }

    #[test]
    fn a_proposal_is_usable_before_it_is_reviewed() {
        // The whole reason review is not a gate: a trainer mid-programme is
        // not blocked waiting for a head coach to notice.
        assert!(made_by_coach("Landmine Press").status.is_prescribable());
    }

    #[test]
    fn approving_moves_a_proposal_into_the_catalogue() {
        let mut ex = made_by_coach("Pendlay Row");
        ex.approve().unwrap();
        assert_eq!(ex.status, CatalogueStatus::Approved);
    }

    #[test]
    fn approving_twice_is_refused() {
        let mut ex = made_by_coach("Pendlay Row");
        ex.approve().unwrap();
        assert!(ex.approve().is_err());
    }

    #[test]
    fn a_retired_movement_cannot_be_newly_prescribed() {
        let mut ex = made_by_coach("Upright Row");
        ex.retire().unwrap();
        assert!(!ex.status.is_prescribable());
    }

    #[test]
    fn retiring_is_reversible_but_not_by_approve() {
        let mut ex = made_by_coach("Good Morning");
        ex.retire().unwrap();
        assert!(
            ex.approve().is_err(),
            "approve is for proposals; a retired movement is reinstated"
        );
        ex.reinstate().unwrap();
        assert_eq!(ex.status, CatalogueStatus::Approved);
    }

    #[test]
    fn reinstating_something_live_is_refused() {
        let mut ex = made_by_coach("Front Squat");
        assert!(ex.reinstate().is_err());
    }
}

//! Programme assignments — a published version reaching a person.
//!
//! This is the join that turns authored content into coaching. Two rules carry
//! all the weight:
//!
//! **An assignment pins a specific version, never "the programme"** (ADR-0006).
//! When the coach later publishes v2, members on v1 stay on v1 until someone
//! deliberately moves them. If assignments pointed at the programme, publishing
//! would silently change what every member is doing mid-week.
//!
//! **Only a published version is assignable.** A draft is a coach's workbench,
//! and in-review is frozen for a reviewer — neither is a plan a member should be
//! executing. The database enforces this too (migration 0010), because the app
//! is not the only thing that can write.
//!
//! Assignments end (withdrawn or completed); they are never deleted. "What was I
//! assigned in March?" must stay answerable, because performed workouts will hang
//! off it.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    ids::{AssignmentId, GymId, ProgramId, ProgramVersionId, UserId},
    program::ProgramVersion,
};

/// How far from today a start date may be. Wide enough for real planning
/// ("start after the holidays"), tight enough to catch a fat-fingered year.
const MAX_START_OFFSET_DAYS: i64 = 366;

/// Where an assignment stands. Same evidence-carrying shape as `VersionStatus`:
/// "withdrawn, but by nobody" is not a state anyone should have to handle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AssignmentStatus {
    Active,
    /// The programme was finished. Set from execution data (Phase 3); no API
    /// route drives it yet.
    Completed {
        completed_at: DateTime<Utc>,
    },
    /// Taken off the programme before finishing — coach's decision, reassignment,
    /// or the member leaving.
    Withdrawn {
        withdrawn_at: DateTime<Utc>,
        withdrawn_by: UserId,
    },
}

impl AssignmentStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed { .. } => "completed",
            Self::Withdrawn { .. } => "withdrawn",
        }
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AssignmentError {
    #[error("only a published programme version can be assigned")]
    NotAssignable,

    #[error("start date must be within a year of today")]
    StartDateOutOfRange,

    #[error("this assignment is already {status}")]
    NotActive { status: &'static str },
}

/// One athlete, on one specific version of one programme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProgramAssignment {
    pub id: AssignmentId,
    pub gym_id: GymId,
    pub athlete_id: UserId,
    /// Denormalised from the version so "one active assignment per programme per
    /// athlete" is enforceable without a join. The database checks they agree.
    pub program_id: ProgramId,
    pub program_version_id: ProgramVersionId,
    pub assigned_by: UserId,
    /// A wall-clock date, not an instant — "starts Monday" means the athlete's
    /// Monday, and the version's content is keyed by week and day anyway.
    pub start_date: NaiveDate,
    pub status: AssignmentStatus,
    pub created_at: DateTime<Utc>,
}

impl ProgramAssignment {
    /// Assign a version to an athlete.
    ///
    /// Takes the whole `ProgramVersion` rather than an id so the assignability
    /// check cannot be skipped — you cannot construct an assignment without
    /// having the version in hand.
    pub fn new(
        athlete_id: UserId,
        version: &ProgramVersion,
        assigned_by: UserId,
        start_date: NaiveDate,
        today: NaiveDate,
        now: DateTime<Utc>,
    ) -> Result<Self, AssignmentError> {
        if !version.status.is_assignable() {
            return Err(AssignmentError::NotAssignable);
        }

        let offset = (start_date - today).num_days().abs();
        if offset > MAX_START_OFFSET_DAYS {
            return Err(AssignmentError::StartDateOutOfRange);
        }

        Ok(Self {
            id: AssignmentId::new(),
            gym_id: version.gym_id,
            athlete_id,
            program_id: version.program_id,
            program_version_id: version.id,
            assigned_by,
            start_date,
            status: AssignmentStatus::Active,
            created_at: now,
        })
    }

    /// Take the athlete off the programme. Records who and when; never deletes.
    pub fn withdraw(
        &mut self,
        withdrawn_by: UserId,
        now: DateTime<Utc>,
    ) -> Result<(), AssignmentError> {
        self.ensure_active()?;
        self.status = AssignmentStatus::Withdrawn {
            withdrawn_at: now,
            withdrawn_by,
        };
        Ok(())
    }

    /// Mark the programme finished. Driven by execution data once Phase 3's
    /// logging exists; modelled now so the storage does not need migrating then.
    pub fn complete(&mut self, now: DateTime<Utc>) -> Result<(), AssignmentError> {
        self.ensure_active()?;
        self.status = AssignmentStatus::Completed { completed_at: now };
        Ok(())
    }

    fn ensure_active(&self) -> Result<(), AssignmentError> {
        if self.status.is_active() {
            return Ok(());
        }
        Err(AssignmentError::NotActive {
            status: self.status.as_str(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::{ApprovalPolicy, Program, ProgramVersion};

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 7, 18).unwrap()
    }

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn published_version() -> ProgramVersion {
        let program = Program::new(
            GymId::new(),
            "Beginner Strength",
            None,
            crate::program::ProgramFocus::Strength,
            UserId::new(),
            now(),
        )
        .unwrap();
        let author = program.created_by;
        let mut version = ProgramVersion::first(&program, author, now());
        version.submit_for_review(1, author, now()).unwrap();
        version
            .approve(UserId::new(), ApprovalPolicy::RequireSecondPerson, now())
            .unwrap();
        version.publish(UserId::new(), now()).unwrap();
        version
    }

    #[test]
    fn assigns_a_published_version() {
        let version = published_version();
        let assignment = ProgramAssignment::new(
            UserId::new(),
            &version,
            UserId::new(),
            today(),
            today(),
            now(),
        )
        .unwrap();

        assert!(assignment.status.is_active());
        // The anchor of ADR-0006: the assignment pins the VERSION.
        assert_eq!(assignment.program_version_id, version.id);
        assert_eq!(assignment.program_id, version.program_id);
        assert_eq!(assignment.gym_id, version.gym_id);
    }

    #[test]
    fn refuses_every_unpublished_state() {
        let program = Program::new(
            GymId::new(),
            "Draft Only",
            None,
            crate::program::ProgramFocus::General,
            UserId::new(),
            now(),
        )
        .unwrap();
        let author = program.created_by;

        // Draft.
        let mut version = ProgramVersion::first(&program, author, now());
        let attempt = |v: &ProgramVersion| {
            ProgramAssignment::new(UserId::new(), v, UserId::new(), today(), today(), now())
        };
        assert_eq!(
            attempt(&version).unwrap_err(),
            AssignmentError::NotAssignable
        );

        // In review — frozen for a reviewer is still not a member's plan.
        version.submit_for_review(1, author, now()).unwrap();
        assert_eq!(
            attempt(&version).unwrap_err(),
            AssignmentError::NotAssignable
        );

        // Approved but not yet published.
        version
            .approve(UserId::new(), ApprovalPolicy::RequireSecondPerson, now())
            .unwrap();
        assert_eq!(
            attempt(&version).unwrap_err(),
            AssignmentError::NotAssignable
        );

        // Archived — retired versions serve existing assignments, never new ones.
        version.publish(UserId::new(), now()).unwrap();
        version.archive(UserId::new(), now()).unwrap();
        assert_eq!(
            attempt(&version).unwrap_err(),
            AssignmentError::NotAssignable
        );
    }

    #[test]
    fn allows_reasonable_start_dates_and_refuses_typos() {
        let version = published_version();
        let attempt = |start: NaiveDate| {
            ProgramAssignment::new(
                UserId::new(),
                &version,
                UserId::new(),
                start,
                today(),
                now(),
            )
        };

        // Backdating a week and planning a month ahead are both normal coaching.
        assert!(attempt(today() - chrono::Duration::days(7)).is_ok());
        assert!(attempt(today() + chrono::Duration::days(30)).is_ok());

        // A mistyped year is not.
        assert_eq!(
            attempt(NaiveDate::from_ymd_opt(2062, 7, 18).unwrap()).unwrap_err(),
            AssignmentError::StartDateOutOfRange
        );
    }

    #[test]
    fn withdrawing_records_who_and_when() {
        let version = published_version();
        let mut assignment = ProgramAssignment::new(
            UserId::new(),
            &version,
            UserId::new(),
            today(),
            today(),
            now(),
        )
        .unwrap();

        let coach = UserId::new();
        assignment.withdraw(coach, now()).unwrap();

        match assignment.status {
            AssignmentStatus::Withdrawn { withdrawn_by, .. } => assert_eq!(withdrawn_by, coach),
            ref other => panic!("expected withdrawn, got {other:?}"),
        }
    }

    #[test]
    fn a_finished_assignment_cannot_be_withdrawn_and_vice_versa() {
        let version = published_version();
        let make = || {
            ProgramAssignment::new(
                UserId::new(),
                &version,
                UserId::new(),
                today(),
                today(),
                now(),
            )
            .unwrap()
        };

        let mut done = make();
        done.complete(now()).unwrap();
        assert_eq!(
            done.withdraw(UserId::new(), now()).unwrap_err(),
            AssignmentError::NotActive {
                status: "completed"
            }
        );

        let mut gone = make();
        gone.withdraw(UserId::new(), now()).unwrap();
        assert_eq!(
            gone.complete(now()).unwrap_err(),
            AssignmentError::NotActive {
                status: "withdrawn"
            }
        );
    }

    #[test]
    fn status_round_trips_through_json() {
        let version = published_version();
        let mut assignment = ProgramAssignment::new(
            UserId::new(),
            &version,
            UserId::new(),
            today(),
            today(),
            now(),
        )
        .unwrap();
        assignment.withdraw(UserId::new(), now()).unwrap();

        let json = serde_json::to_string(&assignment.status).unwrap();
        assert!(json.contains("\"state\":\"withdrawn\""), "got {json}");
        assert_eq!(
            serde_json::from_str::<AssignmentStatus>(&json).unwrap(),
            assignment.status
        );
    }
}

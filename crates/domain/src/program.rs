//! Programmes and their versions — the heart of the product (ADR-0006).
//!
//! The one rule everything else defends: **a published version never changes.**
//! A member assigned version 2 must still be able to see, months later, exactly
//! what version 2 told them to do. If a coach could edit a published plan, they
//! would retroactively rewrite history and every piece of longitudinal data built
//! on it becomes uninterpretable.
//!
//! So editing a published version is not an edit. It creates a **new draft**, and
//! existing assignments keep pointing at the version they were given.
//!
//! The lifecycle is modelled as an enum carrying the data each state actually has,
//! rather than a `status: String` beside a pile of nullable `published_at` /
//! `published_by` columns. That makes "published but with no publisher" — a state
//! the nullable version can represent and code has to keep re-checking —
//! impossible to write down.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    DomainError,
    ids::{GymId, ProgramId, ProgramVersionId, UserId},
    validated_name,
};

/// What a programme is FOR. One word, coach-chosen, load-bearing: this is what
/// lets recommendations be a deterministic rule ("a lift goal suggests strength
/// programmes") instead of a model guessing from prose. Coarse on purpose —
/// four buckets a coach can pick without a taxonomy debate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProgramFocus {
    Strength,
    Hypertrophy,
    Conditioning,
    General,
}

impl ProgramFocus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Strength => "strength",
            Self::Hypertrophy => "hypertrophy",
            Self::Conditioning => "conditioning",
            Self::General => "general",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "strength" => Self::Strength,
            "hypertrophy" => Self::Hypertrophy,
            "conditioning" => Self::Conditioning,
            "general" => Self::General,
            _ => return None,
        })
    }

    /// Does a trainer's free-text specialty speak to this focus?
    ///
    /// Case-insensitive substring over a small keyword list — deliberately dumb.
    /// A ranking model would be unexplainable; "their profile says
    /// 'Powerlifting'" is a reason a member can read and judge.
    #[must_use]
    pub fn matches_specialty(&self, specialty: &str) -> bool {
        let s = specialty.to_lowercase();
        let keywords: &[&str] = match self {
            Self::Strength => &["strength", "powerlifting", "barbell", "lifting"],
            Self::Hypertrophy => &["hypertrophy", "bodybuilding", "muscle", "physique"],
            Self::Conditioning => &[
                "conditioning",
                "endurance",
                "cardio",
                "weight loss",
                "fat loss",
                "running",
            ],
            Self::General => &["general", "beginner", "foundation"],
        };
        keywords.iter().any(|k| s.contains(k))
    }
}

/// Where a version sits in `Draft → In review → Approved → Published → Archived`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum VersionStatus {
    /// The only mutable state. Content may be added, changed and removed freely.
    Draft,
    InReview {
        submitted_at: DateTime<Utc>,
        submitted_by: UserId,
    },
    Approved {
        approved_at: DateTime<Utc>,
        approved_by: UserId,
    },
    /// Immutable. Assignments point here.
    Published {
        published_at: DateTime<Utc>,
        published_by: UserId,
    },
    /// Retired from use. Still readable, because assignments may reference it.
    Archived {
        archived_at: DateTime<Utc>,
        archived_by: UserId,
    },
}

impl VersionStatus {
    /// Short name, for errors, audit records and storage.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::InReview { .. } => "in_review",
            Self::Approved { .. } => "approved",
            Self::Published { .. } => "published",
            Self::Archived { .. } => "archived",
        }
    }

    /// May the version's *content* be changed?
    ///
    /// Only a draft. In-review is frozen so a reviewer is not reading a moving
    /// target, and everything past it is frozen for good.
    #[must_use]
    pub const fn is_mutable(&self) -> bool {
        matches!(self, Self::Draft)
    }

    /// Can a member be assigned this version?
    #[must_use]
    pub const fn is_assignable(&self) -> bool {
        matches!(self, Self::Published { .. })
    }
}

/// Who may approve a version for publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalPolicy {
    /// Someone other than the author must approve. The default for a real gym:
    /// the review gate exists precisely so a second pair of eyes sees the work.
    RequireSecondPerson,
    /// The author may approve their own work. Necessary for a personal gym —
    /// a solo coach has no second person, and blocking them would make the
    /// feature unusable for an entire tier of user rather than safer.
    AllowSelfApproval,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum LifecycleError {
    #[error("a {status} version cannot be edited — create a new draft from it instead")]
    NotEditable { status: &'static str },

    #[error("cannot {action} a version that is {status}")]
    InvalidTransition {
        action: &'static str,
        status: &'static str,
    },

    #[error("a version needs at least one prescribed exercise before it can be reviewed — a week or a workout with nothing in it is not trainable")]
    NoContent,

    #[error("a version must be approved by someone other than its author")]
    SelfApproval,

    #[error("only a published version can be used as the basis for a new draft")]
    NotPublished,
}

/// The stable identity a member is coached on.
///
/// Deliberately holds no training content. "Beginner Strength" is the programme;
/// what it *prescribes* lives in versions, which is what makes the content
/// versionable without the member's assignment losing its anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Program {
    pub id: ProgramId,
    pub gym_id: GymId,
    pub name: String,
    #[schema(nullable)]
    pub summary: Option<String>,
    pub focus: ProgramFocus,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
}

impl Program {
    pub fn new(
        gym_id: GymId,
        name: &str,
        summary: Option<&str>,
        focus: ProgramFocus,
        created_by: UserId,
        now: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        let summary = match summary.map(str::trim) {
            None | Some("") => None,
            Some(text) => Some(validated_name("summary", text, 2000)?),
        };

        Ok(Self {
            id: ProgramId::new(),
            gym_id,
            name: validated_name("name", name, 160)?,
            summary,
            focus,
            created_by,
            created_at: now,
        })
    }
}

/// One snapshot of a programme's content, and its position in the lifecycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProgramVersion {
    pub id: ProgramVersionId,
    pub program_id: ProgramId,
    pub gym_id: GymId,
    /// 1, 2, 3… within a programme. Stable and human-quotable ("she's on v2").
    pub version_number: i32,
    pub status: VersionStatus,
    pub created_by: UserId,
    pub created_at: DateTime<Utc>,
    /// Set when this draft was created from an existing published version, so the
    /// lineage of a programme stays visible.
    #[schema(nullable)]
    pub derived_from: Option<ProgramVersionId>,
}

impl ProgramVersion {
    /// The first version of a new programme. Always a draft.
    #[must_use]
    pub fn first(program: &Program, created_by: UserId, now: DateTime<Utc>) -> Self {
        Self {
            id: ProgramVersionId::new(),
            program_id: program.id,
            gym_id: program.gym_id,
            version_number: 1,
            status: VersionStatus::Draft,
            created_by,
            created_at: now,
            derived_from: None,
        }
    }

    /// Begin a new draft from this **published** version.
    ///
    /// This is what "editing a published programme" actually is. The published
    /// version is untouched and keeps serving every assignment already pointing
    /// at it; the returned draft is a separate row that may be freely changed.
    pub fn new_draft_from(
        &self,
        next_version_number: i32,
        created_by: UserId,
        now: DateTime<Utc>,
    ) -> Result<Self, LifecycleError> {
        if !matches!(self.status, VersionStatus::Published { .. }) {
            // Branching from a draft or an in-review version would fork work in
            // progress and leave two editable versions racing each other.
            return Err(LifecycleError::NotPublished);
        }

        Ok(Self {
            id: ProgramVersionId::new(),
            program_id: self.program_id,
            gym_id: self.gym_id,
            version_number: next_version_number,
            status: VersionStatus::Draft,
            created_by,
            created_at: now,
            derived_from: Some(self.id),
        })
    }

    /// Guard for any content mutation. Call before touching weeks or workouts.
    pub fn ensure_editable(&self) -> Result<(), LifecycleError> {
        if self.status.is_mutable() {
            return Ok(());
        }
        Err(LifecycleError::NotEditable {
            status: self.status.as_str(),
        })
    }

    /// Draft → In review.
    ///
    /// The count checked here is **prescribed exercises**, not weeks. Weeks and
    /// workouts are containers; what makes a version trainable is the exercises
    /// inside them. Counting weeks let two unusable versions through review and
    /// all the way to published — where content is frozen for good, so neither
    /// could ever be repaired:
    ///
    ///   - 1 week, 0 workouts        → an athlete has nothing to start
    ///   - 1 week, 1 workout, 0 exercises → the session opens on a blank screen
    ///
    /// An exercise count of zero catches both, and any version with at least one
    /// prescribed exercise necessarily has a workout and a week to hold it.
    pub fn submit_for_review(
        &mut self,
        prescribed_exercise_count: usize,
        submitted_by: UserId,
        now: DateTime<Utc>,
    ) -> Result<(), LifecycleError> {
        if !matches!(self.status, VersionStatus::Draft) {
            return Err(LifecycleError::InvalidTransition {
                action: "submit for review",
                status: self.status.as_str(),
            });
        }
        if prescribed_exercise_count == 0 {
            return Err(LifecycleError::NoContent);
        }

        self.status = VersionStatus::InReview {
            submitted_at: now,
            submitted_by,
        };
        Ok(())
    }

    /// In review → Approved.
    pub fn approve(
        &mut self,
        approved_by: UserId,
        policy: ApprovalPolicy,
        now: DateTime<Utc>,
    ) -> Result<(), LifecycleError> {
        if !matches!(self.status, VersionStatus::InReview { .. }) {
            return Err(LifecycleError::InvalidTransition {
                action: "approve",
                status: self.status.as_str(),
            });
        }
        if policy == ApprovalPolicy::RequireSecondPerson && approved_by == self.created_by {
            return Err(LifecycleError::SelfApproval);
        }

        self.status = VersionStatus::Approved {
            approved_at: now,
            approved_by,
        };
        Ok(())
    }

    /// In review → Draft. The reviewer wants changes, or the author withdrew.
    pub fn return_to_draft(&mut self) -> Result<(), LifecycleError> {
        if !matches!(self.status, VersionStatus::InReview { .. }) {
            return Err(LifecycleError::InvalidTransition {
                action: "return to draft",
                status: self.status.as_str(),
            });
        }
        self.status = VersionStatus::Draft;
        Ok(())
    }

    /// Approved → Published. **After this the content is frozen for good.**
    pub fn publish(
        &mut self,
        published_by: UserId,
        now: DateTime<Utc>,
    ) -> Result<(), LifecycleError> {
        if !matches!(self.status, VersionStatus::Approved { .. }) {
            // Publishing straight from draft would bypass the review gate, which
            // is the entire reason the intermediate states exist.
            return Err(LifecycleError::InvalidTransition {
                action: "publish",
                status: self.status.as_str(),
            });
        }

        self.status = VersionStatus::Published {
            published_at: now,
            published_by,
        };
        Ok(())
    }

    /// Retire a version. Reachable from anywhere except archived.
    ///
    /// Archiving is not deletion: assignments may still reference this version,
    /// and a member's history has to stay readable.
    pub fn archive(
        &mut self,
        archived_by: UserId,
        now: DateTime<Utc>,
    ) -> Result<(), LifecycleError> {
        if matches!(self.status, VersionStatus::Archived { .. }) {
            return Err(LifecycleError::InvalidTransition {
                action: "archive",
                status: self.status.as_str(),
            });
        }

        self.status = VersionStatus::Archived {
            archived_at: now,
            archived_by,
        };
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn program() -> Program {
        Program::new(
            GymId::new(),
            "Beginner Strength",
            None,
            ProgramFocus::Strength,
            UserId::new(),
            now(),
        )
        .unwrap()
    }

    /// Walk a version to published, returning it and its author.
    fn published() -> (ProgramVersion, UserId) {
        let p = program();
        let author = p.created_by;
        let mut v = ProgramVersion::first(&p, author, now());
        v.submit_for_review(3, author, now()).unwrap();
        v.approve(UserId::new(), ApprovalPolicy::RequireSecondPerson, now())
            .unwrap();
        v.publish(UserId::new(), now()).unwrap();
        (v, author)
    }

    #[test]
    fn a_new_programme_starts_as_draft_version_one() {
        let p = program();
        let v = ProgramVersion::first(&p, p.created_by, now());
        assert_eq!(v.version_number, 1);
        assert_eq!(v.status, VersionStatus::Draft);
        assert!(v.derived_from.is_none());
    }

    #[test]
    fn only_a_draft_is_editable() {
        let p = program();
        let mut v = ProgramVersion::first(&p, p.created_by, now());
        assert!(v.ensure_editable().is_ok());

        // Frozen during review so the reviewer is not reading a moving target.
        v.submit_for_review(1, p.created_by, now()).unwrap();
        assert!(v.ensure_editable().is_err());
    }

    #[test]
    fn a_published_version_can_never_be_edited() {
        let (v, _) = published();
        assert_eq!(
            v.ensure_editable(),
            Err(LifecycleError::NotEditable {
                status: "published"
            })
        );
    }

    #[test]
    fn an_empty_version_cannot_be_sent_for_review() {
        let p = program();
        let mut v = ProgramVersion::first(&p, p.created_by, now());
        assert_eq!(
            v.submit_for_review(0, p.created_by, now()),
            Err(LifecycleError::NoContent)
        );
        assert_eq!(v.status, VersionStatus::Draft, "must not change state");
    }

    /// The count is **prescribed exercises**, and this is the regression that
    /// forced it. Both shapes below have weeks — the old week-count gate waved
    /// them through, they were approved, published (frozen for good) and
    /// assigned, and the athlete got a blank screen with no way back:
    ///
    ///   - a week holding no workouts at all
    ///   - a week holding a workout that prescribes nothing
    ///
    /// Neither is trainable, so neither is reviewable. A container is not content.
    #[test]
    fn a_version_whose_weeks_prescribe_nothing_cannot_be_sent_for_review() {
        let p = program();
        for containers in [1_usize, 5] {
            let mut v = ProgramVersion::first(&p, p.created_by, now());
            assert_eq!(
                v.submit_for_review(0, p.created_by, now()),
                Err(LifecycleError::NoContent),
                "{containers} week(s) of empty containers still prescribes nothing"
            );
            assert_eq!(v.status, VersionStatus::Draft, "must not change state");
        }
    }

    /// The other side of the gate: one real prescription is enough, and it
    /// implies the week and workout that hold it.
    #[test]
    fn a_single_prescribed_exercise_is_enough_content_to_review() {
        let p = program();
        let mut v = ProgramVersion::first(&p, p.created_by, now());
        assert!(v.submit_for_review(1, p.created_by, now()).is_ok());
        assert!(matches!(v.status, VersionStatus::InReview { .. }));
    }

    #[test]
    fn publishing_cannot_skip_the_review_gate() {
        let p = program();
        let mut v = ProgramVersion::first(&p, p.created_by, now());

        // Straight from draft — the whole point of the intermediate states.
        assert!(v.publish(UserId::new(), now()).is_err());

        v.submit_for_review(2, p.created_by, now()).unwrap();
        // Still not approved.
        assert!(v.publish(UserId::new(), now()).is_err());
    }

    #[test]
    fn the_author_cannot_approve_their_own_work_in_a_staffed_gym() {
        let p = program();
        let author = p.created_by;
        let mut v = ProgramVersion::first(&p, author, now());
        v.submit_for_review(2, author, now()).unwrap();

        assert_eq!(
            v.approve(author, ApprovalPolicy::RequireSecondPerson, now()),
            Err(LifecycleError::SelfApproval)
        );
    }

    #[test]
    fn a_solo_coach_may_approve_their_own_work() {
        // Otherwise the entire feature is unusable in a personal gym, which
        // would make the rule harmful rather than safe.
        let p = program();
        let author = p.created_by;
        let mut v = ProgramVersion::first(&p, author, now());
        v.submit_for_review(2, author, now()).unwrap();

        assert!(
            v.approve(author, ApprovalPolicy::AllowSelfApproval, now())
                .is_ok()
        );
    }

    #[test]
    fn a_reviewer_can_send_work_back() {
        let p = program();
        let mut v = ProgramVersion::first(&p, p.created_by, now());
        v.submit_for_review(2, p.created_by, now()).unwrap();

        v.return_to_draft().unwrap();
        assert_eq!(v.status, VersionStatus::Draft);
        // ...and it becomes editable again, which is the point of sending it back.
        assert!(v.ensure_editable().is_ok());
    }

    #[test]
    fn editing_a_published_version_produces_a_new_draft_and_leaves_it_untouched() {
        let (v1, _) = published();
        let before = v1.clone();

        let v2 = v1.new_draft_from(2, UserId::new(), now()).unwrap();

        assert_eq!(v1, before, "the published version must not be mutated");
        assert_eq!(v2.status, VersionStatus::Draft);
        assert_eq!(v2.version_number, 2);
        assert_eq!(v2.derived_from, Some(v1.id));
        assert_ne!(v2.id, v1.id);
        assert_eq!(v2.program_id, v1.program_id, "same programme identity");
    }

    #[test]
    fn only_a_published_version_can_seed_a_new_draft() {
        // Branching from work in progress would leave two editable versions
        // racing each other.
        let p = program();
        let draft = ProgramVersion::first(&p, p.created_by, now());
        assert_eq!(
            draft.new_draft_from(2, UserId::new(), now()).unwrap_err(),
            LifecycleError::NotPublished
        );
    }

    #[test]
    fn only_published_versions_are_assignable() {
        let p = program();
        let mut v = ProgramVersion::first(&p, p.created_by, now());
        assert!(!v.status.is_assignable());

        v.submit_for_review(1, p.created_by, now()).unwrap();
        assert!(!v.status.is_assignable());

        v.approve(UserId::new(), ApprovalPolicy::RequireSecondPerson, now())
            .unwrap();
        assert!(!v.status.is_assignable(), "approved is not yet published");

        v.publish(UserId::new(), now()).unwrap();
        assert!(v.status.is_assignable());
    }

    #[test]
    fn archiving_a_published_version_keeps_it_readable_but_unassignable() {
        let (mut v, _) = published();
        v.archive(UserId::new(), now()).unwrap();

        assert!(!v.status.is_assignable());
        assert!(!v.status.is_mutable(), "archiving must not reopen editing");
    }

    #[test]
    fn a_version_cannot_be_archived_twice() {
        let (mut v, _) = published();
        v.archive(UserId::new(), now()).unwrap();
        assert!(v.archive(UserId::new(), now()).is_err());
    }

    #[test]
    fn status_carries_its_own_evidence() {
        // The reason for the enum: there is no way to be published without a
        // publisher and a timestamp, so no code downstream has to re-check.
        let (v, _) = published();
        match v.status {
            VersionStatus::Published {
                published_at,
                published_by,
            } => {
                assert!(published_at <= Utc::now());
                assert_ne!(published_by, UserId::from(uuid::Uuid::nil()));
            }
            other => panic!("expected published, got {other:?}"),
        }
    }

    #[test]
    fn status_round_trips_through_json() {
        let (v, _) = published();
        let json = serde_json::to_string(&v.status).unwrap();
        assert!(json.contains("\"state\":\"published\""), "got {json}");
        assert_eq!(
            serde_json::from_str::<VersionStatus>(&json).unwrap(),
            v.status
        );
    }
}

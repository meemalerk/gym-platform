//! Workout execution — what actually happened, as opposed to what was planned.
//!
//! The rule inherited from docs/domain-model.md: **prescribed and performed are
//! two separate immutable records. Never collapse them.** The prescription lives
//! in the published version; what the member did lives here. Every progress
//! metric, adherence number and future adjustment reads the gap between the two,
//! and a system that overwrites one with the other has nothing left to compute.
//!
//! Two shapes matter for the offline future (ADR-0008):
//!
//! - **Ids are client-generated.** A phone in a basement gym mints the session
//!   and set ids itself (UUIDv7) and syncs them later; the server accepts an
//!   id it has already seen as a no-op, which is what makes retries and replays
//!   safe. Nothing here assumes the server invented the id.
//! - **Timestamps travel with the data.** `started_at` comes from the client,
//!   because the workout happened when it happened, not when connectivity came
//!   back.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    DomainError,
    assignment::ProgramAssignment,
    ids::{
        AssignmentId, ExerciseId, GymId, PerformedSetId, TemplateExerciseId, UserId,
        WorkoutSessionId, WorkoutTemplateId,
    },
};

/// Bounds. Same philosophy as prescriptions: generous for humans, fatal for typos.
const MAX_REPS: u16 = 500;
const MAX_WEIGHT_KG: f64 = 1000.0;
const MAX_SECONDS: u32 = 36_000;
const MAX_METRES: u32 = 1_000_000;
const MAX_RPE: u8 = 10;
const MAX_SET_NUMBER: i32 = 100;
/// A session may be logged after the fact, but not from another year.
const MAX_STARTED_AT_AGE_DAYS: i64 = 366;
/// What a member may call a session they built themselves. Matches the CHECK
/// on `workout_sessions.title` — a bound stated in two places must be the same
/// number, or one of them is decoration.
const MAX_TITLE_CHARS: usize = 80;

/// Where a session stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SessionStatus {
    InProgress,
    Completed {
        completed_at: DateTime<Utc>,
    },
    /// Started but not finished — cut short, or the member walked away. Kept
    /// distinct from completed because adherence must not count it as done.
    Abandoned {
        abandoned_at: DateTime<Utc>,
    },
}

impl SessionStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::InProgress => "in_progress",
            Self::Completed { .. } => "completed",
            Self::Abandoned { .. } => "abandoned",
        }
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        matches!(self, Self::InProgress)
    }
}

/// What was actually done, shaped like the modality it measures — the performed
/// mirror of `ExercisePrescription`.
///
/// `Repetitions { reps: 0, .. }` is legal and meaningful: a failed set is a
/// real event a coach wants to see, not invalid input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PerformedValue {
    Repetitions {
        reps: u16,
        /// Load, if any — bodyweight work has none. `f64` because plates come in
        /// 1.25s and 2.5s; this is why the enum is `PartialEq`, not `Eq`.
        #[schema(nullable)]
        weight_kg: Option<f64>,
    },
    Duration {
        seconds: u32,
    },
    Distance {
        metres: u32,
    },
}

impl PerformedValue {
    /// Re-check bounds on an already-built value — serde skips constructors, so
    /// this is called by the only path to persistence, same as prescriptions.
    pub fn validate(&self) -> Result<(), DomainError> {
        match self {
            Self::Repetitions { reps, weight_kg } => {
                if *reps > MAX_REPS {
                    return Err(DomainError::Invalid(format!(
                        "reps must be at most {MAX_REPS}"
                    )));
                }
                if let Some(w) = weight_kg
                    && (!w.is_finite() || *w <= 0.0 || *w > MAX_WEIGHT_KG)
                {
                    return Err(DomainError::Invalid(format!(
                        "weight must be between 0 and {MAX_WEIGHT_KG} kg"
                    )));
                }
            }
            Self::Duration { seconds } => {
                if *seconds == 0 || *seconds > MAX_SECONDS {
                    return Err(DomainError::Invalid(format!(
                        "duration must be between 1 and {MAX_SECONDS} seconds"
                    )));
                }
            }
            Self::Distance { metres } => {
                if *metres == 0 || *metres > MAX_METRES {
                    return Err(DomainError::Invalid(format!(
                        "distance must be between 1 and {MAX_METRES} metres"
                    )));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ExecutionError {
    #[error("a session can only be logged against your own active assignment")]
    NotYourAssignment,

    #[error("the assignment is not active")]
    AssignmentNotActive,

    #[error("started_at must be within a year of now, and not in the future")]
    StartedAtOutOfRange,

    #[error("a session name must be at most {MAX_TITLE_CHARS} characters")]
    TitleTooLong,

    #[error("this session is already {status}")]
    SessionNotOpen { status: &'static str },
}

/// One visit to the gym: either against one workout of the assigned version,
/// or — for a member training on their own — against nothing at all.
///
/// The plan link is **both or neither** (ADR-0035). A session with an
/// assignment executes a prescription and every adherence number reads it; a
/// session without one is an *open session*, built by the member as they go.
/// The founding rule is unchanged either way — prescribed and performed stay
/// two records — an open session simply has no prescribed half, which is more
/// honest than inventing an empty plan to point at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WorkoutSession {
    /// Client-generated (ADR-0008). The server accepts a repeat as a no-op.
    pub id: WorkoutSessionId,
    pub gym_id: GymId,
    pub athlete_id: UserId,
    /// The assignment being executed. `None` for an open session.
    #[schema(nullable)]
    pub assignment_id: Option<AssignmentId>,
    /// The workout within the assigned version. `None` for an open session,
    /// and never one without the other — see `is_open_session`.
    #[schema(nullable)]
    pub workout_template_id: Option<WorkoutTemplateId>,
    /// What the member called it. Only an open session has one: a planned
    /// session is named by its workout template, and copying that name here
    /// would go stale the moment a new version renames the workout.
    #[schema(nullable)]
    pub title: Option<String>,
    /// When the workout happened — client wall-clock, possibly hours before sync.
    pub started_at: DateTime<Utc>,
    pub status: SessionStatus,
    /// When the athlete actually stopped training — **client wall-clock**, the
    /// same clock as `started_at`, so the two subtract to a true duration.
    ///
    /// Distinct from the `completed_at` inside `status`, which is when the
    /// SERVER heard about it. Both are kept because they answer different
    /// questions and only one of them is trustworthy for arithmetic: a session
    /// finished offline and synced three hours later has a `completed_at`
    /// three hours late, and "how long did they train for" computed from it is
    /// simply wrong. That was the bug this field exists to fix.
    ///
    /// `None` for sessions logged before this existed, and for open ones.
    /// Callers fall back to `completed_at` rather than showing nothing —
    /// an approximate duration beats a blank for history already recorded.
    pub ended_at: Option<DateTime<Utc>>,
}

impl WorkoutSession {
    /// Start (or backfill) a session against `assignment`.
    ///
    /// Takes the whole assignment so the two checks that matter cannot be
    /// skipped: it must be the athlete's own, and it must still be active.
    pub fn new(
        id: WorkoutSessionId,
        actor: UserId,
        assignment: &ProgramAssignment,
        workout_template_id: WorkoutTemplateId,
        started_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Self, ExecutionError> {
        if assignment.athlete_id != actor {
            return Err(ExecutionError::NotYourAssignment);
        }
        if !assignment.status.is_active() {
            return Err(ExecutionError::AssignmentNotActive);
        }

        check_started_at(started_at, now)?;

        Ok(Self {
            id,
            gym_id: assignment.gym_id,
            athlete_id: actor,
            assignment_id: Some(assignment.id),
            workout_template_id: Some(workout_template_id),
            title: None,
            started_at,
            status: SessionStatus::InProgress,
            ended_at: None,
        })
    }

    /// Start an **unplanned session** — a workout the member builds themselves.
    ///
    /// There is no assignment to check ownership against, so the actor simply
    /// is the athlete: an open session is by construction your own, which is
    /// the same rule as before rather than a relaxation of it. Whether the
    /// member is *entitled* to train is a question for `EntitlementService`,
    /// asked in the application layer where the assigned path asks it too.
    ///
    /// `title` is what they called it. Blank is `None`, not an error — the
    /// member came to train, not to fill in a form, and "Open session" is a
    /// perfectly good name for a workout nobody wanted to name.
    pub fn open(
        id: WorkoutSessionId,
        gym_id: GymId,
        athlete_id: UserId,
        title: Option<&str>,
        started_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Self, ExecutionError> {
        check_started_at(started_at, now)?;

        let title = match title.map(str::trim).filter(|t| !t.is_empty()) {
            None => None,
            Some(t) if t.chars().count() > MAX_TITLE_CHARS => {
                return Err(ExecutionError::TitleTooLong);
            }
            Some(t) => Some(t.to_owned()),
        };

        Ok(Self {
            id,
            gym_id,
            athlete_id,
            assignment_id: None,
            workout_template_id: None,
            title,
            started_at,
            status: SessionStatus::InProgress,
            ended_at: None,
        })
    }

    /// Was this built by the member rather than prescribed to them?
    ///
    /// Deliberately NOT called "open": `status.is_open()` already means "still
    /// in progress" on this very type, and two senses of the same word one
    /// method apart is how somebody eventually writes the wrong one. Unplanned
    /// says the actual thing — there is no plan behind it.
    ///
    /// Reads one half of the pair on purpose: the two are kept both-or-neither
    /// by the constructors here and by a CHECK constraint in the database, so
    /// asking about both would imply a third state that cannot exist.
    #[must_use]
    pub const fn is_unplanned(&self) -> bool {
        self.assignment_id.is_none()
    }

    /// Finish a session.
    ///
    /// `now` is the server clock — the record of when we were told. `ended_at`
    /// is the athlete's clock, and it is the one durations are computed from.
    pub fn complete(
        &mut self,
        now: DateTime<Utc>,
        ended_at: Option<DateTime<Utc>>,
    ) -> Result<(), ExecutionError> {
        self.ensure_open()?;
        self.ended_at = self.validated_end(ended_at, now)?;
        self.status = SessionStatus::Completed { completed_at: now };
        Ok(())
    }

    pub fn abandon(
        &mut self,
        now: DateTime<Utc>,
        ended_at: Option<DateTime<Utc>>,
    ) -> Result<(), ExecutionError> {
        self.ensure_open()?;
        self.ended_at = self.validated_end(ended_at, now)?;
        self.status = SessionStatus::Abandoned { abandoned_at: now };
        Ok(())
    }

    /// How long the athlete trained, if it can be known.
    ///
    /// Prefers the client's own end time; falls back to the server's record for
    /// sessions that predate `ended_at`. `None` while a session is still open —
    /// a running session has an elapsed time, not a duration, and the two are
    /// different enough that conflating them would misreport adherence.
    #[must_use]
    pub fn duration(&self) -> Option<chrono::Duration> {
        let end = self.ended_at.or(match self.status {
            SessionStatus::Completed { completed_at } => Some(completed_at),
            SessionStatus::Abandoned { abandoned_at } => Some(abandoned_at),
            SessionStatus::InProgress => None,
        })?;
        Some(end.signed_duration_since(self.started_at))
    }

    /// A client-supplied end time we are willing to believe.
    ///
    /// Two guards, both learned from `started_at`: a device clock can be wrong,
    /// and a wrong one must not produce a negative or absurd duration that then
    /// poisons every average computed from it. An unusable value is DROPPED
    /// rather than rejected — the session really did finish, and refusing the
    /// whole request would strand an athlete with an un-closable workout
    /// because their phone's clock drifted.
    fn validated_end(
        &self,
        ended_at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, ExecutionError> {
        let Some(end) = ended_at else { return Ok(None) };

        if end < self.started_at {
            return Ok(None);
        }
        if end.signed_duration_since(self.started_at).num_seconds() > i64::from(MAX_SECONDS) {
            return Ok(None);
        }
        // A finish in the future is a clock ahead of ours, not a prediction.
        // Small skew is normal; an hour is not.
        if end.signed_duration_since(now).num_hours() > 1 {
            return Ok(None);
        }
        Ok(Some(end))
    }

    fn ensure_open(&self) -> Result<(), ExecutionError> {
        if self.status.is_open() {
            return Ok(());
        }
        Err(ExecutionError::SessionNotOpen {
            status: self.status.as_str(),
        })
    }
}

/// A `started_at` we are willing to record.
///
/// Five minutes of forward clock skew is tolerated; a session "from tomorrow"
/// is a clock problem the member should see rather than history to store. Both
/// constructors call this, because a rule enforced on one path and not the
/// other is not a rule.
fn check_started_at(started_at: DateTime<Utc>, now: DateTime<Utc>) -> Result<(), ExecutionError> {
    let age = now.signed_duration_since(started_at);
    if age.num_days() > MAX_STARTED_AT_AGE_DAYS || age.num_seconds() < -300 {
        return Err(ExecutionError::StartedAtOutOfRange);
    }
    Ok(())
}

/// One performed set — the atom of training history. Immutable once written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PerformedSet {
    /// Client-generated, like the session's.
    pub id: PerformedSetId,
    pub session_id: WorkoutSessionId,
    pub gym_id: GymId,
    pub exercise_id: ExerciseId,
    /// The prescription line this answers, when it answers one. `None` means
    /// extra work outside the plan — real, and worth keeping distinct.
    #[schema(nullable)]
    pub template_exercise_id: Option<TemplateExerciseId>,
    /// 1-based within (session, exercise).
    pub set_number: i32,
    pub performed: PerformedValue,
    /// Rating of perceived exertion, 0–10, if the member gave one.
    #[schema(nullable)]
    pub rpe: Option<u8>,
}

/// What the member says they did, before it becomes history.
#[derive(Debug, Clone, PartialEq)]
pub struct SetEntry {
    pub exercise_id: ExerciseId,
    pub template_exercise_id: Option<TemplateExerciseId>,
    pub set_number: i32,
    pub performed: PerformedValue,
    pub rpe: Option<u8>,
}

impl PerformedSet {
    /// Log a set into an **open** session belonging to the actor.
    pub fn new(
        id: PerformedSetId,
        session: &WorkoutSession,
        actor: UserId,
        entry: SetEntry,
    ) -> Result<Self, DomainError> {
        // Your history is yours to write — nobody else's.
        if session.athlete_id != actor {
            return Err(DomainError::Invalid(
                "sets can only be logged into your own session".into(),
            ));
        }
        if !session.status.is_open() {
            return Err(DomainError::Invalid(format!(
                "this session is already {}",
                session.status.as_str()
            )));
        }
        if !(1..=MAX_SET_NUMBER).contains(&entry.set_number) {
            return Err(DomainError::Invalid(format!(
                "set number must be between 1 and {MAX_SET_NUMBER}"
            )));
        }
        if let Some(rpe) = entry.rpe
            && rpe > MAX_RPE
        {
            return Err(DomainError::Invalid(format!(
                "RPE must be at most {MAX_RPE}"
            )));
        }
        entry.performed.validate()?;

        Ok(Self {
            id,
            session_id: session.id,
            gym_id: session.gym_id,
            exercise_id: entry.exercise_id,
            template_exercise_id: entry.template_exercise_id,
            set_number: entry.set_number,
            performed: entry.performed,
            rpe: entry.rpe,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::{ApprovalPolicy, Program, ProgramVersion};

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn assignment_for(athlete: UserId) -> ProgramAssignment {
        let program = Program::new(
            GymId::new(),
            "P",
            None,
            crate::program::ProgramFocus::General,
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

        ProgramAssignment::new(
            athlete,
            &version,
            UserId::new(),
            now().date_naive(),
            now().date_naive(),
            now(),
        )
        .unwrap()
    }

    fn open_session(athlete: UserId) -> WorkoutSession {
        WorkoutSession::new(
            WorkoutSessionId::new(),
            athlete,
            &assignment_for(athlete),
            WorkoutTemplateId::new(),
            now(),
            now(),
        )
        .unwrap()
    }

    #[test]
    fn an_athlete_starts_a_session_on_their_own_assignment() {
        let athlete = UserId::new();
        let session = open_session(athlete);
        assert!(session.status.is_open());
        assert_eq!(session.athlete_id, athlete);
    }

    #[test]
    fn nobody_starts_a_session_on_someone_elses_assignment() {
        // Not even a coach: your training history is written by you.
        let athlete = UserId::new();
        let coach = UserId::new();
        let err = WorkoutSession::new(
            WorkoutSessionId::new(),
            coach,
            &assignment_for(athlete),
            WorkoutTemplateId::new(),
            now(),
            now(),
        );
        assert_eq!(err.unwrap_err(), ExecutionError::NotYourAssignment);
    }

    #[test]
    fn a_withdrawn_assignment_takes_no_new_sessions() {
        let athlete = UserId::new();
        let mut assignment = assignment_for(athlete);
        assignment.withdraw(UserId::new(), now()).unwrap();

        let err = WorkoutSession::new(
            WorkoutSessionId::new(),
            athlete,
            &assignment,
            WorkoutTemplateId::new(),
            now(),
            now(),
        );
        assert_eq!(err.unwrap_err(), ExecutionError::AssignmentNotActive);
    }

    #[test]
    fn backfilled_sessions_are_fine_but_future_ones_are_a_clock_problem() {
        let athlete = UserId::new();
        let assignment = assignment_for(athlete);
        let attempt = |started: DateTime<Utc>| {
            WorkoutSession::new(
                WorkoutSessionId::new(),
                athlete,
                &assignment,
                WorkoutTemplateId::new(),
                started,
                now(),
            )
        };

        // Logged after the fact — the offline case (ADR-0008).
        assert!(attempt(now() - chrono::Duration::days(3)).is_ok());
        // Small forward skew tolerated; a session from tomorrow is not.
        assert!(attempt(now() + chrono::Duration::minutes(2)).is_ok());
        assert!(attempt(now() + chrono::Duration::days(1)).is_err());
        assert!(attempt(now() - chrono::Duration::days(400)).is_err());
    }

    #[test]
    fn duration_prefers_the_athletes_clock() {
        // The bug this whole field exists for: a session finished offline and
        // synced hours later must report the time TRAINED, not the time until
        // the phone found signal.
        let mut session = open_session(UserId::new());
        let started = session.started_at;
        let really_ended = started + chrono::Duration::minutes(52);
        let heard_about_it = started + chrono::Duration::hours(4);

        session
            .complete(heard_about_it, Some(really_ended))
            .unwrap();

        assert_eq!(session.duration().unwrap().num_minutes(), 52);
    }

    #[test]
    fn duration_falls_back_to_the_server_clock_for_old_history() {
        // Sessions recorded before ended_at existed have no honest end. An
        // approximate duration beats a blank for history that was mostly
        // logged online anyway.
        let mut session = open_session(UserId::new());
        let started = session.started_at;
        session
            .complete(started + chrono::Duration::minutes(45), None)
            .unwrap();

        assert_eq!(session.duration().unwrap().num_minutes(), 45);
        assert!(session.ended_at.is_none());
    }

    #[test]
    fn an_open_session_has_no_duration() {
        // It has an ELAPSED time, which is a different thing — conflating them
        // would let an abandoned-but-never-closed session count as a 30-hour
        // workout in any average.
        assert!(open_session(UserId::new()).duration().is_none());
    }

    #[test]
    fn an_impossible_end_time_is_dropped_not_refused() {
        // A drifting phone clock must never leave an athlete unable to close
        // their workout — so a bad value is discarded and the session still
        // finishes, falling back to the server's time.
        let started = open_session(UserId::new()).started_at;

        for bad in [
            started - chrono::Duration::hours(1), // before it began
            started + chrono::Duration::days(30), // absurdly long
            now() + chrono::Duration::hours(6),   // clock far ahead
        ] {
            let mut session = open_session(UserId::new());
            session.started_at = started;
            session.complete(now(), Some(bad)).unwrap();
            assert!(
                session.ended_at.is_none(),
                "an impossible end time must be dropped, not stored: {bad}"
            );
            assert!(session.duration().is_some(), "the session still finished");
        }
    }

    #[test]
    fn a_plausible_end_time_is_kept() {
        let mut session = open_session(UserId::new());
        let end = session.started_at + chrono::Duration::minutes(70);
        session.complete(now(), Some(end)).unwrap();
        assert_eq!(session.ended_at, Some(end));
    }

    #[test]
    fn abandoning_records_a_duration_too() {
        // Someone who trained for ten minutes and gave up trained for ten
        // minutes. Adherence must not count it as complete, but the time is
        // still real.
        let mut session = open_session(UserId::new());
        let end = session.started_at + chrono::Duration::minutes(10);
        session.abandon(now(), Some(end)).unwrap();
        assert_eq!(session.duration().unwrap().num_minutes(), 10);
        assert_eq!(session.status.as_str(), "abandoned");
    }

    #[test]
    fn finishing_a_session_freezes_it() {
        let athlete = UserId::new();
        let mut session = open_session(athlete);
        session.complete(now(), None).unwrap();

        assert!(session.abandon(now(), None).is_err(), "completed is final");
        assert!(
            PerformedSet::new(
                PerformedSetId::new(),
                &session,
                athlete,
                SetEntry {
                    exercise_id: ExerciseId::new(),
                    template_exercise_id: None,
                    set_number: 1,
                    performed: PerformedValue::Repetitions {
                        reps: 5,
                        weight_kg: None
                    },
                    rpe: None,
                },
            )
            .is_err(),
            "no sets into a finished session"
        );
    }

    #[test]
    fn logs_a_set_with_weight_and_rpe() {
        let athlete = UserId::new();
        let session = open_session(athlete);
        let set = PerformedSet::new(
            PerformedSetId::new(),
            &session,
            athlete,
            SetEntry {
                exercise_id: ExerciseId::new(),
                template_exercise_id: Some(TemplateExerciseId::new()),
                set_number: 1,
                performed: PerformedValue::Repetitions {
                    reps: 8,
                    weight_kg: Some(62.5),
                },
                rpe: Some(8),
            },
        )
        .unwrap();
        assert_eq!(set.set_number, 1);
    }

    #[test]
    fn a_failed_set_of_zero_reps_is_valid_history() {
        // The set happened; the bar did not go up. That is data, not an error.
        let athlete = UserId::new();
        let session = open_session(athlete);
        assert!(
            PerformedSet::new(
                PerformedSetId::new(),
                &session,
                athlete,
                SetEntry {
                    exercise_id: ExerciseId::new(),
                    template_exercise_id: None,
                    set_number: 3,
                    performed: PerformedValue::Repetitions {
                        reps: 0,
                        weight_kg: Some(100.0)
                    },
                    rpe: Some(10),
                },
            )
            .is_ok()
        );
    }

    #[test]
    fn only_the_athlete_writes_their_history() {
        let athlete = UserId::new();
        let session = open_session(athlete);
        assert!(
            PerformedSet::new(
                PerformedSetId::new(),
                &session,
                UserId::new(),
                SetEntry {
                    exercise_id: ExerciseId::new(),
                    template_exercise_id: None,
                    set_number: 1,
                    performed: PerformedValue::Duration { seconds: 45 },
                    rpe: None,
                },
            )
            .is_err()
        );
    }

    #[test]
    fn deserialised_values_are_still_bounded() {
        // Same hole as prescriptions had: serde skips constructors.
        let absurd: PerformedValue =
            serde_json::from_str(r#"{"kind":"repetitions","reps":9999}"#).unwrap();
        assert!(absurd.validate().is_err());

        let negative: PerformedValue =
            serde_json::from_str(r#"{"kind":"repetitions","reps":5,"weight_kg":-20}"#).unwrap();
        assert!(negative.validate().is_err());

        let fine: PerformedValue =
            serde_json::from_str(r#"{"kind":"distance","metres":5000}"#).unwrap();
        assert!(fine.validate().is_ok());
    }

    #[test]
    fn rejects_impossible_rpe_and_set_numbers() {
        let athlete = UserId::new();
        let session = open_session(athlete);
        let attempt = |set_number: i32, rpe: Option<u8>| {
            PerformedSet::new(
                PerformedSetId::new(),
                &session,
                athlete,
                SetEntry {
                    exercise_id: ExerciseId::new(),
                    template_exercise_id: None,
                    set_number,
                    performed: PerformedValue::Duration { seconds: 60 },
                    rpe,
                },
            )
        };

        assert!(attempt(0, None).is_err());
        assert!(attempt(101, None).is_err());
        assert!(attempt(1, Some(11)).is_err());
        assert!(attempt(1, Some(10)).is_ok());
    }

    #[test]
    fn status_round_trips_through_json() {
        let athlete = UserId::new();
        let mut session = open_session(athlete);
        session.abandon(now(), None).unwrap();

        let json = serde_json::to_string(&session.status).unwrap();
        assert!(json.contains("\"state\":\"abandoned\""), "got {json}");
        assert_eq!(
            serde_json::from_str::<SessionStatus>(&json).unwrap(),
            session.status
        );
    }

    // ------------------------------------------------- unplanned sessions

    fn unplanned(athlete: UserId, title: Option<&str>) -> Result<WorkoutSession, ExecutionError> {
        WorkoutSession::open(
            WorkoutSessionId::new(),
            GymId::new(),
            athlete,
            title,
            now(),
            now(),
        )
    }

    #[test]
    fn a_member_with_no_assignment_can_still_start_a_session() {
        let athlete = UserId::new();
        let session = unplanned(athlete, Some("Push day")).unwrap();

        assert!(session.is_unplanned());
        assert_eq!(session.assignment_id, None);
        assert_eq!(session.workout_template_id, None);
        assert_eq!(session.title.as_deref(), Some("Push day"));
        assert_eq!(session.athlete_id, athlete);
        assert!(session.status.is_open(), "a new session is in progress");
    }

    #[test]
    fn an_assigned_session_is_not_unplanned() {
        let athlete = UserId::new();
        let session = open_session(athlete);

        assert!(!session.is_unplanned());
        assert!(session.assignment_id.is_some());
        assert!(session.workout_template_id.is_some());
        assert_eq!(session.title, None, "a prescribed workout names itself");
    }

    #[test]
    fn a_blank_name_is_no_name_rather_than_an_error() {
        // The member came to train, not to fill in a form.
        for blank in ["", "   ", "	
"] {
            let session = unplanned(UserId::new(), Some(blank)).unwrap();
            assert_eq!(session.title, None, "{blank:?} should normalise away");
        }
        assert_eq!(unplanned(UserId::new(), None).unwrap().title, None);
    }

    #[test]
    fn a_name_is_trimmed_and_bounded() {
        let session = unplanned(UserId::new(), Some("  Leg day  ")).unwrap();
        assert_eq!(session.title.as_deref(), Some("Leg day"));

        let just_fits = "x".repeat(MAX_TITLE_CHARS);
        assert!(unplanned(UserId::new(), Some(&just_fits)).is_ok());

        let one_too_many = "x".repeat(MAX_TITLE_CHARS + 1);
        assert_eq!(
            unplanned(UserId::new(), Some(&one_too_many)),
            Err(ExecutionError::TitleTooLong)
        );
    }

    #[test]
    fn an_unplanned_session_obeys_the_same_clock_rules() {
        // The guard is shared with the assigned path, so this pins that it is
        // actually shared rather than merely similar.
        let long_ago = now() - chrono::Duration::days(MAX_STARTED_AT_AGE_DAYS + 1);
        assert_eq!(
            WorkoutSession::open(
                WorkoutSessionId::new(),
                GymId::new(),
                UserId::new(),
                None,
                long_ago,
                now()
            ),
            Err(ExecutionError::StartedAtOutOfRange)
        );

        let tomorrow = now() + chrono::Duration::days(1);
        assert_eq!(
            WorkoutSession::open(
                WorkoutSessionId::new(),
                GymId::new(),
                UserId::new(),
                None,
                tomorrow,
                now()
            ),
            Err(ExecutionError::StartedAtOutOfRange)
        );
    }

    #[test]
    fn sets_log_into_an_unplanned_session_with_no_prescription_line() {
        // The whole feature rests on this: `template_exercise_id` has always
        // been optional ("work outside the plan"), so a session that is ALL
        // work outside the plan needs no new set machinery.
        let athlete = UserId::new();
        let session = unplanned(athlete, Some("Chest")).unwrap();

        let set = PerformedSet::new(
            PerformedSetId::new(),
            &session,
            athlete,
            SetEntry {
                exercise_id: ExerciseId::new(),
                template_exercise_id: None,
                set_number: 1,
                performed: PerformedValue::Repetitions {
                    reps: 8,
                    weight_kg: Some(60.0),
                },
                rpe: Some(8),
            },
        )
        .unwrap();

        assert_eq!(set.session_id, session.id);
        assert_eq!(set.template_exercise_id, None);
        assert_eq!(set.gym_id, session.gym_id);
    }

    #[test]
    fn an_unplanned_session_is_still_only_writable_by_its_own_athlete() {
        // No assignment to check ownership against, so this is the only guard
        // left — worth pinning that it did not go missing with the plan link.
        let athlete = UserId::new();
        let session = unplanned(athlete, None).unwrap();

        let intruder = PerformedSet::new(
            PerformedSetId::new(),
            &session,
            UserId::new(),
            SetEntry {
                exercise_id: ExerciseId::new(),
                template_exercise_id: None,
                set_number: 1,
                performed: PerformedValue::Duration { seconds: 60 },
                rpe: None,
            },
        );
        assert!(intruder.is_err());
    }

    #[test]
    fn an_unplanned_session_finishes_and_reports_a_duration() {
        let athlete = UserId::new();
        let started = now() - chrono::Duration::minutes(45);
        let mut session = WorkoutSession::open(
            WorkoutSessionId::new(),
            GymId::new(),
            athlete,
            Some("Back"),
            started,
            now(),
        )
        .unwrap();

        assert_eq!(session.duration(), None, "open sessions have elapsed time");
        session.complete(now(), Some(started + chrono::Duration::minutes(45)))
            .unwrap();

        assert_eq!(session.duration().unwrap().num_minutes(), 45);
        assert!(!session.status.is_open());
    }
}

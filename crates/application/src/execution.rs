//! Workout execution use-cases.
//!
//! The authority here inverts the assignment rule, and the inversion is the
//! point: **only the athlete writes their own history.** A coach decides what
//! you *should* do (assignment); only you can say what you *did*. Coaches and
//! managers read; nobody else writes.
//!
//! Ids arrive from the client and inserts are idempotent (ADR-0008) — "created"
//! and "replayed" both succeed, so an offline phone can retry a sync forever
//! without duplicating history.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use gym_domain::{
    AssignmentId, ExerciseId, PerformedSetId, TemplateExerciseId, TenantContext, UserId,
    WorkoutSessionId, WorkoutTemplateId,
    entitlement::Feature,
    execution::{ExecutionError, PerformedSet, PerformedValue, SetEntry, WorkoutSession},
};

use crate::{
    ApplicationError, ApplicationResult,
    ports::{
        AssignmentRepository, Clock, CoachRepository, ExecutionRepository, ProgramRepository,
        SessionFilter, SessionView,
    },
};

#[derive(Debug, Clone)]
pub struct StartSessionCommand {
    /// Client-generated (UUIDv7 minted on the phone). Replaying it is a no-op.
    pub id: WorkoutSessionId,
    /// The assignment being executed, or `None` for an unplanned session the
    /// member builds themselves (ADR-0035). Both-or-neither with
    /// `workout_template_id`; one without the other is refused.
    pub assignment_id: Option<AssignmentId>,
    pub workout_template_id: Option<WorkoutTemplateId>,
    /// What the member called an unplanned session. Ignored — and refused —
    /// when a plan link is present, because a prescribed workout is named by
    /// its template.
    pub title: Option<String>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct LogSetCommand {
    pub id: PerformedSetId,
    pub session_id: WorkoutSessionId,
    pub exercise_id: ExerciseId,
    pub template_exercise_id: Option<TemplateExerciseId>,
    pub set_number: i32,
    pub performed: PerformedValue,
    pub rpe: Option<u8>,
}

/// How a session ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishOutcome {
    Completed,
    Abandoned,
}

#[derive(Clone)]
pub struct ExecutionService {
    pub sessions: Arc<dyn ExecutionRepository>,
    pub assignments: Arc<dyn AssignmentRepository>,
    pub programs: Arc<dyn ProgramRepository>,
    pub relationships: Arc<dyn CoachRepository>,
    /// Consulted before a session starts. Every feature gate in the codebase
    /// asks this service, so switching billing on later is one file.
    pub entitlements: crate::entitlements::EntitlementService,
    pub clock: Arc<dyn Clock>,
}

impl ExecutionService {
    /// Sessions the caller may see: the gym's for a manager, their clients' and
    /// their own for everyone else — the same scoping as assignments.
    pub async fn list(
        &self,
        tenant: &TenantContext,
        filter: &SessionFilter,
    ) -> ApplicationResult<Vec<SessionView>> {
        let all = self.sessions.list(tenant, filter).await?;

        if tenant.capabilities.can_manage_catalogue() {
            return Ok(all);
        }

        let coached: Vec<UserId> = self
            .relationships
            .active_for_user(tenant, tenant.actor_id)
            .await?
            .into_iter()
            .filter(|r| r.coach_id == tenant.actor_id)
            .map(|r| r.athlete_id)
            .collect();

        Ok(all
            .into_iter()
            .filter(|v| {
                v.session.athlete_id == tenant.actor_id || coached.contains(&v.session.athlete_id)
            })
            .collect())
    }

    /// One session with its sets — the read a logging screen sits on.
    ///
    /// Returns the display projection, not the bare domain session: the screen
    /// that renders this is the one being trained on, and it has to be able to
    /// name the workout.
    pub async fn session_with_sets(
        &self,
        tenant: &TenantContext,
        id: WorkoutSessionId,
    ) -> ApplicationResult<(SessionView, Vec<PerformedSet>)> {
        let view = self
            .sessions
            .find_session_view(tenant, id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "session" })?;

        // Read authority: yourself, your coach, or a manager. Everyone else
        // sees "not found" — a session id must not confirm a session exists.
        if view.session.athlete_id != tenant.actor_id
            && !tenant.capabilities.can_manage_catalogue()
            && !self
                .relationships
                .active_for_user(tenant, tenant.actor_id)
                .await?
                .iter()
                .any(|r| r.grants_access_to(tenant.actor_id, view.session.athlete_id))
        {
            return Err(ApplicationError::NotFound { entity: "session" });
        }

        let sets = self.sessions.sets_of(tenant, id).await?;
        Ok((view, sets))
    }

    /// Start a session — or acknowledge one this phone already started.
    pub async fn start(
        &self,
        tenant: &TenantContext,
        cmd: StartSessionCommand,
    ) -> ApplicationResult<(WorkoutSession, bool)> {
        // Idempotent replay: the id is already ours? Return it unchanged.
        if let Some(existing) = self.sessions.find_session(tenant, cmd.id).await? {
            if existing.athlete_id == tenant.actor_id {
                return Ok((existing, false));
            }
            // Same id, different athlete: either a UUID collision (effectively
            // impossible) or someone replaying another person's capture.
            return Err(ApplicationError::Conflict("this session id".to_owned()));
        }

        // Feature gate. Asked here rather than at the route so every caller —
        // including a future offline replay — goes through it. Today it grants
        // for any gym that does not sell plans, and for anyone whose plan
        // includes gym access; when dunning and suspension arrive they change
        // `EntitlementService`, not this line
        // (feature-plan-2026-07.md §6).
        self.entitlements
            .require(tenant, tenant.actor_id, Feature::GymAccess)
            .await?;

        // Unplanned: nothing to look up, nothing to pin, nothing to contradict.
        // The entitlement check above is the whole of the authority question —
        // which is the point of asking it above the branch rather than inside
        // the assigned path, where an unplanned session would have skipped it.
        let Some(assignment_id) = cmd.assignment_id else {
            if cmd.workout_template_id.is_some() {
                return Err(ApplicationError::Domain(gym_domain::DomainError::Invalid(
                    "a workout without an assignment is not a session anyone was given".into(),
                )));
            }

            let session = WorkoutSession::open(
                cmd.id,
                tenant.gym_id,
                tenant.actor_id,
                cmd.title.as_deref(),
                cmd.started_at,
                self.clock.now(),
            )?;
            let created = self.sessions.insert_session(tenant, &session).await?;
            return Ok((session, created));
        };

        // Planned from here down. A name belongs to the workout template, so
        // accepting one here would create a second source for the same string.
        let Some(workout_template_id) = cmd.workout_template_id else {
            return Err(ApplicationError::Domain(gym_domain::DomainError::Invalid(
                "starting an assigned session needs the workout being done".into(),
            )));
        };
        if cmd.title.is_some() {
            return Err(ApplicationError::Domain(gym_domain::DomainError::Invalid(
                "an assigned session is named by its workout, not by the athlete".into(),
            )));
        }

        let assignment = self.assignments.find(tenant, assignment_id).await?.ok_or(
            ApplicationError::NotFound {
                entity: "assignment",
            },
        )?;

        // The workout must belong to the assignment's pinned version — else the
        // session claims to execute a plan the athlete was never given. The
        // database trigger re-checks this.
        let workout = self
            .programs
            .find_workout(tenant, workout_template_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "workout" })?;
        let week = self
            .programs
            .find_week(tenant, workout.week_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "week" })?;
        if week.version_id != assignment.program_version_id {
            return Err(ApplicationError::Domain(gym_domain::DomainError::Invalid(
                "this workout is not part of the assigned programme version".into(),
            )));
        }

        let session = WorkoutSession::new(
            cmd.id,
            tenant.actor_id,
            &assignment,
            workout_template_id,
            cmd.started_at,
            self.clock.now(),
        )?;

        let created = self.sessions.insert_session(tenant, &session).await?;
        Ok((session, created))
    }

    /// Log one performed set. Idempotent on the set id.
    pub async fn log_set(
        &self,
        tenant: &TenantContext,
        cmd: LogSetCommand,
    ) -> ApplicationResult<PerformedSet> {
        let session = self
            .sessions
            .find_session(tenant, cmd.session_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "session" })?;

        // The domain constructor enforces: your session, still open, sane values.
        let set = PerformedSet::new(
            cmd.id,
            &session,
            tenant.actor_id,
            SetEntry {
                exercise_id: cmd.exercise_id,
                template_exercise_id: cmd.template_exercise_id,
                set_number: cmd.set_number,
                performed: cmd.performed,
                rpe: cmd.rpe,
            },
        )?;

        self.sessions.insert_set(tenant, &set).await?;
        Ok(set)
    }

    /// One exercise's history for one athlete — self, their coach, or a manager.
    pub async fn exercise_history(
        &self,
        tenant: &TenantContext,
        exercise: ExerciseId,
        athlete: UserId,
    ) -> ApplicationResult<Vec<crate::ports::ExerciseHistoryEntry>> {
        // Same read gate as a single session: anyone else gets "not found",
        // never confirmation that this athlete trains here.
        if athlete != tenant.actor_id
            && !tenant.capabilities.can_manage_catalogue()
            && !self
                .relationships
                .active_for_user(tenant, tenant.actor_id)
                .await?
                .iter()
                .any(|r| r.grants_access_to(tenant.actor_id, athlete))
        {
            return Err(ApplicationError::NotFound { entity: "history" });
        }

        self.sessions
            .exercise_history(tenant, exercise, athlete)
            .await
    }

    /// Finish a session, one way or the other.
    ///
    /// `ended_at` is the athlete's own clock and is what durations are computed
    /// from; the server clock still records when it heard. A value that cannot
    /// be true — before the start, absurdly long, or an hour into our future —
    /// is dropped rather than refused, because an athlete with a drifting phone
    /// clock must still be able to close their workout.
    pub async fn finish(
        &self,
        tenant: &TenantContext,
        id: WorkoutSessionId,
        outcome: FinishOutcome,
        ended_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> ApplicationResult<WorkoutSession> {
        let mut session = self
            .sessions
            .find_session(tenant, id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "session" })?;

        if session.athlete_id != tenant.actor_id {
            // Your history is yours to close, too.
            return Err(ApplicationError::Forbidden);
        }

        let now = self.clock.now();
        let action = match outcome {
            FinishOutcome::Completed => {
                session.complete(now, ended_at)?;
                "workout_session.completed"
            }
            FinishOutcome::Abandoned => {
                session.abandon(now, ended_at)?;
                "workout_session.abandoned"
            }
        };

        self.sessions
            .save_finished(tenant, &session, action)
            .await?;
        Ok(session)
    }
}

impl From<ExecutionError> for ApplicationError {
    fn from(err: ExecutionError) -> Self {
        match err {
            ExecutionError::NotYourAssignment => Self::Forbidden,
            ExecutionError::SessionNotOpen { .. } => Self::Conflict("this session".to_owned()),
            other => Self::Domain(gym_domain::DomainError::Invalid(other.to_string())),
        }
    }
}

//! Goal use-cases.
//!
//! The authority is deliberately looser than assignment: **a member may set
//! their own goals.** Deciding your programming is your coach's job; deciding
//! what you are chasing is yours. A coach may also set goals for their clients,
//! and either may confirm or abandon — a goal is a shared artefact of the
//! coaching conversation, not an instruction.

use std::sync::Arc;

use chrono::NaiveDate;
use gym_domain::{
    GoalId, TenantContext, UserId,
    coaching::may_coach_athlete,
    goal::{Goal, GoalMetric},
};

use crate::{
    ApplicationError, ApplicationResult,
    ports::{Clock, CoachRepository, ExerciseRepository, GoalRepository, GoalView, UserRepository},
};

#[derive(Debug, Clone)]
pub struct CreateGoalCommand {
    pub athlete_id: UserId,
    pub metric: GoalMetric,
    pub target_date: Option<NaiveDate>,
}

/// How a goal closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalOutcome {
    Achieved,
    Abandoned,
}

#[derive(Clone)]
pub struct GoalService {
    pub goals: Arc<dyn GoalRepository>,
    pub exercises: Arc<dyn ExerciseRepository>,
    pub relationships: Arc<dyn CoachRepository>,
    pub users: Arc<dyn UserRepository>,
    pub clock: Arc<dyn Clock>,
}

impl GoalService {
    /// Goals the caller may see: the gym's for a manager, their clients' and
    /// their own for everyone else.
    pub async fn list(&self, tenant: &TenantContext) -> ApplicationResult<Vec<GoalView>> {
        let all = self.goals.list(tenant).await?;

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
                v.goal.athlete_id == tenant.actor_id || coached.contains(&v.goal.athlete_id)
            })
            .collect())
    }

    /// Set a goal — your own, or a client's.
    pub async fn create(
        &self,
        tenant: &TenantContext,
        cmd: CreateGoalCommand,
    ) -> ApplicationResult<Goal> {
        self.ensure_may_touch(tenant, cmd.athlete_id).await?;

        // The athlete must be a current member; not-found rather than
        // unsuitable, as everywhere.
        if self
            .users
            .capabilities_in(cmd.athlete_id, tenant.gym_id)
            .await?
            .is_empty()
        {
            return Err(ApplicationError::NotFound { entity: "member" });
        }

        // A lift goal names an exercise; it must exist in THIS gym. The
        // tenant-scoped lookup makes another gym's exercise read as not-found.
        if let GoalMetric::ExerciseEst1Rm { exercise_id, .. } = &cmd.metric {
            self.exercises
                .find(tenant, *exercise_id)
                .await?
                .ok_or(ApplicationError::NotFound { entity: "exercise" })?;
        }

        let now = self.clock.now();
        let goal = Goal::new(
            tenant.gym_id,
            cmd.athlete_id,
            tenant.actor_id,
            cmd.metric,
            cmd.target_date,
            now.date_naive(),
            now,
        )?;

        self.goals.insert(tenant, &goal).await?;
        Ok(goal)
    }

    /// Close a goal, one way or the other.
    pub async fn close(
        &self,
        tenant: &TenantContext,
        id: GoalId,
        outcome: GoalOutcome,
    ) -> ApplicationResult<Goal> {
        let mut goal = self
            .goals
            .find(tenant, id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "goal" })?;

        self.ensure_may_touch(tenant, goal.athlete_id).await?;

        // 409, not 400: a well-formed request against an already-closed goal is
        // a state conflict — refresh, don't rewrite. Matches every other
        // already-ended path in the codebase.
        if !goal.status.is_active() {
            return Err(ApplicationError::Conflict("this goal".to_owned()));
        }

        let now = self.clock.now();
        let action = match outcome {
            GoalOutcome::Achieved => {
                goal.achieve(tenant.actor_id, now)?;
                "goal.achieved"
            }
            GoalOutcome::Abandoned => {
                goal.abandon(tenant.actor_id, now)?;
                "goal.abandoned"
            }
        };

        self.goals.save_closed(tenant, &goal, action).await?;
        Ok(goal)
    }

    /// Self, their coach, or a manager. The one place self-service is the
    /// point: your goals are yours to set and to give up on.
    async fn ensure_may_touch(
        &self,
        tenant: &TenantContext,
        athlete: UserId,
    ) -> ApplicationResult<()> {
        if athlete == tenant.actor_id || tenant.capabilities.can_manage_catalogue() {
            return Ok(());
        }

        let relationships = self
            .relationships
            .active_for_user(tenant, tenant.actor_id)
            .await?;

        if may_coach_athlete(
            tenant.actor_id,
            &tenant.capabilities,
            athlete,
            &relationships,
        ) {
            return Ok(());
        }
        Err(ApplicationError::Forbidden)
    }
}

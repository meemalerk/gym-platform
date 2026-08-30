//! Programme assignment use-cases — where coaching authority is first exercised.
//!
//! The authority rule is the interesting part: a trainer may assign programmes
//! **to their own clients**, which is what the coach–athlete relationship exists
//! to permit. Managers may assign to anyone. An athlete may assign to nobody,
//! including themselves — deciding your programming is your coach's job.
//!
//! A member training solo therefore has no assignment made *for* them, and that
//! is the correct outcome, not a gap: under ADR-0023 there are no personal gyms
//! to be the manager of any more. Self-directed training reaches a programme by
//! recommendation, and anyone who wants one chosen for them asks for a coach.

use std::sync::Arc;

use chrono::NaiveDate;
use gym_domain::{
    AssignmentId, ProgramVersionId, TenantContext, UserId,
    assignment::{AssignmentError, ProgramAssignment},
};

use crate::{
    ApplicationError, ApplicationResult,
    ports::{
        AssignmentRepository, AssignmentView, Clock, CoachRepository, ProgramRepository,
        UserRepository,
    },
};

#[derive(Debug, Clone)]
pub struct AssignProgramCommand {
    pub athlete_id: UserId,
    pub program_version_id: ProgramVersionId,
    pub start_date: NaiveDate,
}

#[derive(Clone)]
pub struct AssignmentService {
    pub assignments: Arc<dyn AssignmentRepository>,
    pub programs: Arc<dyn ProgramRepository>,
    pub relationships: Arc<dyn CoachRepository>,
    pub users: Arc<dyn UserRepository>,
    pub clock: Arc<dyn Clock>,
}

impl AssignmentService {
    /// The assignments the caller may see: everything for a manager; your
    /// clients' and your own for everyone else.
    pub async fn list(&self, tenant: &TenantContext) -> ApplicationResult<Vec<AssignmentView>> {
        let all = self.assignments.list(tenant).await?;

        if tenant.capabilities.can_manage_catalogue() {
            return Ok(all);
        }

        // One relationship query, then set membership — not one query per row.
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
                v.assignment.athlete_id == tenant.actor_id
                    || coached.contains(&v.assignment.athlete_id)
            })
            .collect())
    }

    /// Put an athlete on a published programme version.
    pub async fn assign(
        &self,
        tenant: &TenantContext,
        cmd: AssignProgramCommand,
    ) -> ApplicationResult<ProgramAssignment> {
        // 1. Authorization: manager, or the athlete's active coach.
        self.ensure_may_prescribe_for(tenant, cmd.athlete_id)
            .await?;

        // The athlete must be a current member. "Not found" rather than
        // "unsuitable", so other gyms' membership cannot be probed.
        if self
            .users
            .capabilities_in(cmd.athlete_id, tenant.gym_id)
            .await?
            .is_empty()
        {
            return Err(ApplicationError::NotFound { entity: "member" });
        }

        // 2. Domain validation. The constructor takes the whole version, so the
        //    published check cannot be skipped; the tenant-scoped lookup means a
        //    version from another gym reads as not-found.
        let version = self
            .programs
            .find_version(tenant, cmd.program_version_id)
            .await?
            .ok_or(ApplicationError::NotFound {
                entity: "program version",
            })?;

        let now = self.clock.now();
        let assignment = ProgramAssignment::new(
            cmd.athlete_id,
            &version,
            tenant.actor_id,
            cmd.start_date,
            now.date_naive(),
            now,
        )?;

        // 3. Persist.
        self.assignments.insert(tenant, &assignment).await?;
        Ok(assignment)
    }

    /// Take an athlete off a programme. Same authority as assigning.
    ///
    /// Which now includes an athlete taking themselves off one — see
    /// `ensure_may_prescribe_for`. Deliberate and symmetrical: a member who can
    /// choose a programme can stop following it, including one a coach set. The
    /// withdrawal is audited like any other, so the coach sees it happened
    /// rather than finding a silently abandoned plan.
    pub async fn withdraw(
        &self,
        tenant: &TenantContext,
        id: AssignmentId,
    ) -> ApplicationResult<ProgramAssignment> {
        let mut assignment =
            self.assignments
                .find(tenant, id)
                .await?
                .ok_or(ApplicationError::NotFound {
                    entity: "assignment",
                })?;

        self.ensure_may_prescribe_for(tenant, assignment.athlete_id)
            .await?;

        assignment.withdraw(tenant.actor_id, self.clock.now())?;
        self.assignments
            .save_ended(tenant, &assignment, "program_assignment.withdrawn")
            .await?;

        Ok(assignment)
    }

    /// Who may put this athlete on a programme.
    ///
    /// **Exactly two answers, and a manager is not one of them (ADR-0034):**
    ///
    ///  1. **The athlete themselves.** Somebody who joined on their own has no
    ///     coach, and without this they could read the whole library and train
    ///     against none of it — the app tracked nothing for them. Narrow on
    ///     purpose (`athlete == actor`), so it grants nothing over anybody else.
    ///  2. **Their own active coach.** Choosing the right programme for the
    ///     person in front of you is coaching, and it is the trainer who knows
    ///     what that person did last week.
    ///
    /// A manager writes and publishes the catalogue (`can_author_programs`) and
    /// decides which trainer works with which member — but does not reach past
    /// the trainer to prescribe. That short-circuit used to be here, and it made
    /// the owner the default prescriber for the whole gym simply because they
    /// could.
    ///
    /// Deliberately NOT `may_coach_athlete`: that function answers "may I SEE
    /// this athlete's data", where a manager genuinely does need to say yes to
    /// run the gym, and it short-circuits on `can_manage_catalogue` for exactly
    /// that reason. Reusing it here is what conflated seeing with prescribing.
    async fn ensure_may_prescribe_for(
        &self,
        tenant: &TenantContext,
        athlete: UserId,
    ) -> ApplicationResult<()> {
        if athlete == tenant.actor_id {
            return Ok(());
        }

        let relationships = self
            .relationships
            .active_for_user(tenant, tenant.actor_id)
            .await?;

        // The relationship, not the capacity — the same distinction the whole
        // coaching model turns on.
        if relationships
            .iter()
            .any(|r| r.grants_access_to(tenant.actor_id, athlete))
        {
            return Ok(());
        }
        Err(ApplicationError::Forbidden)
    }
}

impl From<AssignmentError> for ApplicationError {
    fn from(err: AssignmentError) -> Self {
        match err {
            // Well-formed request, conflicting state — refresh, don't rewrite.
            AssignmentError::NotActive { .. } => Self::Conflict("this assignment".to_owned()),
            other => Self::Domain(gym_domain::DomainError::Invalid(other.to_string())),
        }
    }
}

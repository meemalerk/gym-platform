//! Programme authoring use-cases.
//!
//! Same order as every other handler (docs/authorization-model.md):
//!   1. authorization — "may this actor attempt this?"
//!   2. domain policy — "is this action valid?"
//!   3. persist
//!
//! The lifecycle rules themselves are not here. They live in
//! `gym_domain::program`, so they stay testable without a database and cannot be
//! quietly bypassed by adding a second call site.

use std::sync::Arc;

use gym_domain::{
    ApprovalPolicy, ExerciseId, ProgramId, ProgramVersionId, ProgramWeekId, TenantContext, UserId,
    WorkoutTemplateId,
    prescription::ExercisePrescription,
    program::{LifecycleError, Program, ProgramVersion},
    workout::{ProgramWeek, TemplateExercise, WorkoutTemplate},
};

use crate::{
    ApplicationError, ApplicationResult,
    ports::{
        Clock, ExerciseRepository, GymRepository, ProgramRepository, UserRepository, VersionContent,
    },
};

/// A lifecycle move, named as the API receives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    SubmitForReview,
    Approve,
    ReturnToDraft,
    Publish,
    Archive,
}

impl Transition {
    const fn audit_action(self) -> &'static str {
        match self {
            Self::SubmitForReview => "program_version.submitted",
            Self::Approve => "program_version.approved",
            Self::ReturnToDraft => "program_version.returned_to_draft",
            Self::Publish => "program_version.published",
            Self::Archive => "program_version.archived",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateProgramCommand {
    pub name: String,
    pub summary: Option<String>,
    pub focus: gym_domain::ProgramFocus,
}

#[derive(Debug, Clone)]
pub struct AddWeekCommand {
    pub version_id: ProgramVersionId,
    pub week_number: i32,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AddWorkoutCommand {
    pub week_id: ProgramWeekId,
    pub day_number: i32,
    pub name: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PrescribeExerciseCommand {
    pub workout_id: WorkoutTemplateId,
    pub exercise_id: ExerciseId,
    pub prescription: ExercisePrescription,
    pub notes: Option<String>,
}

#[derive(Clone)]
pub struct ProgramService {
    pub programs: Arc<dyn ProgramRepository>,
    pub exercises: Arc<dyn ExerciseRepository>,
    pub gyms: Arc<dyn GymRepository>,
    /// Only for the approval policy: "is there anybody else who could sign
    /// this off?" is a question about the roster, and the answer decides
    /// whether the second-person rule is a safeguard or a dead end.
    pub users: Arc<dyn UserRepository>,
    pub clock: Arc<dyn Clock>,
}

impl ProgramService {
    /// Anyone in the gym may read programmes. Members need to see what they are
    /// being coached on; hiding the catalogue from them serves nobody.
    pub async fn list(
        &self,
        tenant: &TenantContext,
    ) -> ApplicationResult<Vec<(Program, ProgramVersion)>> {
        self.programs.list(tenant).await
    }

    pub async fn versions(
        &self,
        tenant: &TenantContext,
        program: ProgramId,
    ) -> ApplicationResult<Vec<ProgramVersion>> {
        self.programs.versions(tenant, program).await
    }

    pub async fn content(
        &self,
        tenant: &TenantContext,
        version: ProgramVersionId,
    ) -> ApplicationResult<VersionContent> {
        self.programs
            .load_content(tenant, version)
            .await?
            .ok_or(ApplicationError::NotFound {
                entity: "program version",
            })
    }

    /// Create a programme and its first draft version together.
    ///
    /// Authoring is coach-level (ADR-0024). A draft binds nobody — it is the
    /// review step, not this check, that stands between a programme and the
    /// gym's athletes.
    pub async fn create(
        &self,
        tenant: &TenantContext,
        cmd: CreateProgramCommand,
    ) -> ApplicationResult<(Program, ProgramVersion)> {
        if !tenant.capabilities.can_author_programs() {
            return Err(ApplicationError::Forbidden);
        }

        let now = self.clock.now();
        let program = Program::new(
            tenant.gym_id,
            &cmd.name,
            cmd.summary.as_deref(),
            cmd.focus,
            tenant.actor_id,
            now,
        )?;
        let version = ProgramVersion::first(&program, tenant.actor_id, now);

        self.programs
            .insert_program(tenant, &program, &version)
            .await?;

        Ok((program, version))
    }

    /// Start a new draft from a published version — what "editing a published
    /// programme" actually means (ADR-0006).
    ///
    /// Coach-level, like any other authoring: the new draft is a proposal
    /// against something already published, and it reaches athletes only by
    /// going through review like everything else.
    pub async fn new_draft(
        &self,
        tenant: &TenantContext,
        program_id: ProgramId,
    ) -> ApplicationResult<ProgramVersion> {
        if !tenant.capabilities.can_author_programs() {
            return Err(ApplicationError::Forbidden);
        }

        let versions = self.programs.versions(tenant, program_id).await?;
        if versions.is_empty() {
            return Err(ApplicationError::NotFound { entity: "program" });
        }

        // Branch from the newest published version, not simply the newest: the
        // newest may be an archived experiment, and a coach means "carry on from
        // what members are actually doing".
        let source = versions
            .iter()
            .filter(|v| v.status.is_assignable())
            .max_by_key(|v| v.version_number)
            .ok_or(LifecycleError::NotPublished)?;

        let next_number = versions
            .iter()
            .map(|v| v.version_number)
            .max()
            .unwrap_or(0)
            .saturating_add(1);

        let draft = source.new_draft_from(next_number, tenant.actor_id, self.clock.now())?;
        self.programs.insert_version(tenant, &draft).await?;

        Ok(draft)
    }

    pub async fn add_week(
        &self,
        tenant: &TenantContext,
        cmd: AddWeekCommand,
    ) -> ApplicationResult<ProgramWeek> {
        let version = self.editable_version(tenant, cmd.version_id).await?;

        let week = ProgramWeek::new(version.id, cmd.week_number, cmd.label.as_deref())?;
        self.programs.insert_week(tenant, &week).await?;
        Ok(week)
    }

    pub async fn add_workout(
        &self,
        tenant: &TenantContext,
        cmd: AddWorkoutCommand,
    ) -> ApplicationResult<WorkoutTemplate> {
        let week = self
            .programs
            .find_week(tenant, cmd.week_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "week" })?;

        self.editable_version(tenant, week.version_id).await?;

        let workout =
            WorkoutTemplate::new(week.id, cmd.day_number, &cmd.name, cmd.notes.as_deref())?;
        self.programs.insert_workout(tenant, &workout).await?;
        Ok(workout)
    }

    /// Prescribe an exercise inside a workout.
    ///
    /// Returns the exercise's display name alongside the prescription — the
    /// lookup already happened for the modality check, and making the caller
    /// re-query the catalogue to render what it just created would be waste.
    pub async fn prescribe(
        &self,
        tenant: &TenantContext,
        cmd: PrescribeExerciseCommand,
    ) -> ApplicationResult<(TemplateExercise, String)> {
        let workout = self
            .programs
            .find_workout(tenant, cmd.workout_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "workout" })?;

        let week = self
            .programs
            .find_week(tenant, workout.week_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "week" })?;

        self.editable_version(tenant, week.version_id).await?;

        // The catalogue lookup is tenant-scoped, so an exercise from another gym
        // reads as "not found" rather than leaking that it exists.
        let exercise = self
            .exercises
            .find(tenant, cmd.exercise_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "exercise" })?;

        let position = self.programs.next_position(tenant, workout.id).await?;

        // The constructor checks the prescription suits how this exercise is
        // measured — reps cannot be prescribed for something measured in metres.
        let prescribed = TemplateExercise::new(
            workout.id,
            exercise.id,
            &exercise.modality,
            position,
            cmd.prescription,
            cmd.notes.as_deref(),
        )?;

        self.programs
            .insert_template_exercise(tenant, &prescribed)
            .await?;
        Ok((prescribed, exercise.name))
    }

    /// Move a version through the lifecycle.
    pub async fn transition(
        &self,
        tenant: &TenantContext,
        version_id: ProgramVersionId,
        transition: Transition,
    ) -> ApplicationResult<ProgramVersion> {
        // Lifecycle moves split in two (ADR-0024), along the line of whether
        // the move BINDS THE GYM:
        //
        //   submit / return-to-draft   an author moving their own proposal
        //                              around. Commits nothing.
        //   approve / publish / archive  puts athletes on it, or takes it away.
        //                              Head coach and above.
        //
        // The review gate itself is NOT a capability difference, and reading
        // it as one is the mistake this comment exists to prevent: it is the
        // domain's second-person rule (`ApprovalPolicy::RequireSecondPerson`),
        // which refuses to let anyone approve a version they created. Two head
        // coaches can sign each other's work off; one head coach alone cannot
        // sign off their own, in a gym that has more than one person in it.
        // That rule is what makes opening authoring safe.
        let binds_the_gym = matches!(
            transition,
            Transition::Approve | Transition::Publish | Transition::Archive
        );

        if binds_the_gym {
            if !tenant.capabilities.can_publish_programs() {
                return Err(ApplicationError::Forbidden);
            }
        } else if !tenant.capabilities.can_author_programs() {
            return Err(ApplicationError::Forbidden);
        }

        let mut version = self
            .programs
            .find_version(tenant, version_id)
            .await?
            .ok_or(ApplicationError::NotFound {
                entity: "program version",
            })?;

        // An author may move their OWN draft. Without this, one trainer could
        // submit another's half-written programme for review — the sort of
        // thing that is merely annoying between colleagues and genuinely bad
        // when the draft then gets approved on its author's behalf.
        if !binds_the_gym
            && !tenant.capabilities.can_publish_programs()
            && version.created_by != tenant.actor_id
        {
            return Err(ApplicationError::Forbidden);
        }

        let now = self.clock.now();

        match transition {
            Transition::SubmitForReview => {
                let prescribed = self
                    .programs
                    .prescribed_exercise_count(tenant, version.id)
                    .await?;
                version.submit_for_review(prescribed, tenant.actor_id, now)?;
            }
            Transition::Approve => {
                let policy = self.approval_policy(tenant, version.created_by).await?;
                version.approve(tenant.actor_id, policy, now)?;
            }
            Transition::ReturnToDraft => version.return_to_draft()?,
            Transition::Publish => version.publish(tenant.actor_id, now)?,
            Transition::Archive => version.archive(tenant.actor_id, now)?,
        }

        self.programs
            .save_status(tenant, &version, transition.audit_action())
            .await?;

        Ok(version)
    }

    /// Is there a second person who could sign this off?
    ///
    /// The rule used to be the gym's `is_personal` flag, chosen once at
    /// creation and almost always false — so the ordinary case was a gym with
    /// exactly one owner, who wrote a programme, submitted it for review, and
    /// then could not approve it. Nobody else could either. The lifecycle
    /// stopped dead at "in review" with no legal move left, which is the worst
    /// thing a state machine can do to somebody.
    ///
    /// A second-person rule protects against one person pushing work to the
    /// whole gym unreviewed. Where there IS no second person it protects
    /// against nothing and prevents everything, so it stands down — and the
    /// moment a gym promotes a head coach it comes back, without anyone
    /// changing a setting.
    ///
    /// Note it counts *other people who could publish*, not other members: a
    /// gym full of members still has nobody qualified to review, and pretending
    /// otherwise would be the same dead end wearing a bigger roster.
    async fn approval_policy(
        &self,
        tenant: &TenantContext,
        author: UserId,
    ) -> ApplicationResult<ApprovalPolicy> {
        let gym = self
            .gyms
            .find(tenant.gym_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "gym" })?;

        if gym.is_personal {
            return Ok(ApprovalPolicy::AllowSelfApproval);
        }

        let reviewers = self
            .users
            .roster(tenant)
            .await?
            .into_iter()
            .filter(|m| m.user_id != author && m.capabilities.can_publish_programs())
            .count();

        Ok(if reviewers == 0 {
            ApprovalPolicy::AllowSelfApproval
        } else {
            ApprovalPolicy::RequireSecondPerson
        })
    }

    /// Load a version and confirm the caller may change its content.
    async fn editable_version(
        &self,
        tenant: &TenantContext,
        id: ProgramVersionId,
    ) -> ApplicationResult<ProgramVersion> {
        if !tenant.capabilities.can_author_programs() {
            return Err(ApplicationError::Forbidden);
        }

        let version =
            self.programs
                .find_version(tenant, id)
                .await?
                .ok_or(ApplicationError::NotFound {
                    entity: "program version",
                })?;

        // Authors edit their own drafts; catalogue managers edit anyone's.
        // The distinction only exists below head-coach level — above it, the
        // whole catalogue is the job.
        if !tenant.capabilities.can_publish_programs() && version.created_by != tenant.actor_id {
            return Err(ApplicationError::Forbidden);
        }

        // Refused here with a clear message. The database refuses too — this
        // check is for the human, that one is for correctness.
        version.ensure_editable()?;
        Ok(version)
    }
}

impl From<LifecycleError> for ApplicationError {
    /// Lifecycle violations are the caller asking for something invalid, not a
    /// failure — they map to 4xx, and the message is safe to show.
    fn from(err: LifecycleError) -> Self {
        Self::Domain(gym_domain::DomainError::Invalid(err.to_string()))
    }
}

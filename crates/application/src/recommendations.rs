//! Recommendations — deterministic, explainable, and only ever a suggestion.
//!
//! The rule set is small enough to state in full:
//!
//!   1. Each of YOUR active goals maps to a programme focus
//!      (`GoalMetric::recommended_focus`): a lift goal → strength, a cut →
//!      conditioning, a gain → hypertrophy.
//!   2. Published programmes with that focus, which you are not already on,
//!      are suggested — each carrying the goal that triggered it.
//!   3. Coaches in the gym whose profile specialties speak to that focus
//!      (`ProgramFocus::matches_specialty`), and who do not already coach you,
//!      are suggested — each carrying the specialty that matched.
//!
//! No model, no scoring, no learning. Every suggestion carries a `because` a
//! member can read and judge — which is both the UX rule ("no screen shows a
//! number it cannot explain") and the ADR-0007 posture: deterministic first,
//! and anything smarter arrives as a bounded assistant, not as silent ranking.

use std::sync::Arc;

use gym_domain::{ProgramFocus, TenantContext, UserId, goal::GoalMetric};

use crate::{
    ApplicationResult,
    ports::{
        AssignmentRepository, CoachRepository, GoalRepository, ProfileRepository,
        ProgramRepository, UserRepository,
    },
};

/// A suggested programme, with its reason.
#[derive(Debug, Clone)]
pub struct ProgramSuggestion {
    pub program_id: gym_domain::ProgramId,
    pub name: String,
    pub focus: ProgramFocus,
    /// The published version a coach would assign.
    pub version_id: gym_domain::ProgramVersionId,
    pub because: String,
}

/// A suggested coach, with the specialty that matched.
#[derive(Debug, Clone)]
pub struct TrainerSuggestion {
    pub user_id: UserId,
    pub display_name: String,
    pub headline: Option<String>,
    pub specialties: Vec<String>,
    pub because: String,
}

#[derive(Debug, Clone, Default)]
pub struct Recommendations {
    pub programs: Vec<ProgramSuggestion>,
    pub trainers: Vec<TrainerSuggestion>,
}

#[derive(Clone)]
pub struct RecommendationService {
    pub goals: Arc<dyn GoalRepository>,
    pub programs: Arc<dyn ProgramRepository>,
    pub assignments: Arc<dyn AssignmentRepository>,
    pub relationships: Arc<dyn CoachRepository>,
    pub profiles: Arc<dyn ProfileRepository>,
    pub users: Arc<dyn UserRepository>,
}

impl RecommendationService {
    /// Recommendations for the CALLER. Self-scope only, deliberately: a coach
    /// wondering what suits a client reads the client's goals directly; a
    /// recommendation feed is a personal surface.
    pub async fn for_me(&self, tenant: &TenantContext) -> ApplicationResult<Recommendations> {
        // What am I chasing? No goals, no recommendations — an empty state the
        // client renders as an invitation to set one, not as a shrug.
        let foci: Vec<(ProgramFocus, String)> = self
            .goals
            .list(tenant)
            .await?
            .into_iter()
            .filter(|v| v.goal.athlete_id == tenant.actor_id && v.goal.status.is_active())
            .map(|v| {
                let focus = v.goal.metric.recommended_focus();
                let because = match &v.goal.metric {
                    GoalMetric::Bodyweight {
                        target_kg,
                        baseline_kg,
                    } => {
                        if target_kg < baseline_kg {
                            format!("your goal to cut to {target_kg} kg")
                        } else {
                            format!("your goal to build to {target_kg} kg")
                        }
                    }
                    GoalMetric::ExerciseEst1Rm { target_kg, .. } => {
                        format!("your goal to lift {target_kg} kg")
                    }
                };
                (focus, because)
            })
            .collect();

        if foci.is_empty() {
            return Ok(Recommendations::default());
        }

        let mut out = Recommendations::default();

        // ---- programmes: published, focus-matched, not already assigned ----

        let my_active_programs: Vec<gym_domain::ProgramId> = self
            .assignments
            .list(tenant)
            .await?
            .into_iter()
            .filter(|a| {
                a.assignment.athlete_id == tenant.actor_id && a.assignment.status.is_active()
            })
            .map(|a| a.assignment.program_id)
            .collect();

        for (program, version) in self.programs.list(tenant).await? {
            if !version.status.is_assignable() || my_active_programs.contains(&program.id) {
                continue;
            }
            if let Some((_, because)) = foci.iter().find(|(focus, _)| *focus == program.focus) {
                out.programs.push(ProgramSuggestion {
                    program_id: program.id,
                    name: program.name,
                    focus: program.focus,
                    version_id: version.id,
                    because: format!("Matches {because}"),
                });
            }
        }

        // ---- coaches: specialty-matched, not already coaching me ----

        let my_coaches: Vec<UserId> = self
            .relationships
            .active_for_user(tenant, tenant.actor_id)
            .await?
            .into_iter()
            .filter(|r| r.athlete_id == tenant.actor_id)
            .map(|r| r.coach_id)
            .collect();

        for member in self.users.roster(tenant).await? {
            if member.user_id == tenant.actor_id
                || my_coaches.contains(&member.user_id)
                || !member.capabilities.can_coach()
            {
                continue;
            }
            let Some(profile) = self.profiles.trainer_profile(member.user_id).await? else {
                // No profile, no evidence, no suggestion — never recommend on
                // capacity alone, because there would be nothing to explain.
                continue;
            };

            let matched = foci.iter().find_map(|(focus, _)| {
                profile
                    .specialties
                    .iter()
                    .find(|s| focus.matches_specialty(s))
                    .cloned()
            });

            if let Some(specialty) = matched {
                out.trainers.push(TrainerSuggestion {
                    user_id: member.user_id,
                    display_name: member.display_name,
                    headline: profile.headline,
                    specialties: profile.specialties,
                    because: format!("Their profile lists \u{201c}{specialty}\u{201d}"),
                });
            }
        }

        Ok(out)
    }
}

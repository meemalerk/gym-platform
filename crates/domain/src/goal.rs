//! Goals — a target on a metric the platform can already observe.
//!
//! The line drawn in [feature-plan-2026-07.md]: a goal that cannot be measured
//! is a coach note, not a goal. So the metric is an enum over things the system
//! genuinely records — bodyweight (from measurements) and an exercise's
//! estimated 1RM (from performed sets) — and free-text aspirations live on the
//! athlete profile, deliberately elsewhere, where nothing pretends to track them.
//!
//! Progress is **computed at the edge** against the live series and never stored
//! here. The one number a goal does carry is its **baseline** — the value when
//! the goal was set. That is a fact about a moment, immutable by nature, and
//! without it "70% of the way there" has no denominator.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::{
    DomainError,
    ids::{ExerciseId, GoalId, GymId, UserId},
};

const WEIGHT_BOUNDS: (f64, f64) = (20.0, 500.0);
const LIFT_BOUNDS: (f64, f64) = (1.0, 1000.0);
/// A target more than two years out is a dream, not a training goal.
const MAX_TARGET_HORIZON_DAYS: i64 = 730;

fn check(field: &'static str, value: f64, (min, max): (f64, f64)) -> Result<(), DomainError> {
    if !value.is_finite() || !(min..=max).contains(&value) {
        return Err(DomainError::Invalid(format!(
            "{field} must be between {min} and {max}"
        )));
    }
    Ok(())
}

/// What is being chased, with the baseline captured at creation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GoalMetric {
    /// Bodyweight toward a target — down for a cut, up for a gain. The
    /// direction is derived from baseline vs target, never stated separately,
    /// so the two cannot disagree.
    Bodyweight { baseline_kg: f64, target_kg: f64 },
    /// An exercise's estimated 1RM. The exercise is gym-scoped, which is why
    /// goals are tenant-owned rather than person-owned.
    ///
    /// The rename is load-bearing: serde's snake_case turns `ExerciseEst1Rm`
    /// into `exercise_est1_rm` — digits confuse the case-splitter — and a wire
    /// tag nobody would guess is a wire tag nobody can send.
    #[serde(rename = "exercise_est_1rm")]
    ExerciseEst1Rm {
        exercise_id: ExerciseId,
        baseline_kg: f64,
        target_kg: f64,
    },
}

impl GoalMetric {
    /// Which programme focus this goal suggests — the deterministic heart of
    /// recommendations. A lift goal wants strength work; cutting weight wants
    /// conditioning; gaining wants hypertrophy. One rule, readable, arguable —
    /// which is the point: a member can see WHY something was suggested.
    #[must_use]
    pub fn recommended_focus(&self) -> crate::program::ProgramFocus {
        use crate::program::ProgramFocus;
        match self {
            Self::ExerciseEst1Rm { .. } => ProgramFocus::Strength,
            Self::Bodyweight {
                baseline_kg,
                target_kg,
            } => {
                if target_kg < baseline_kg {
                    ProgramFocus::Conditioning
                } else {
                    ProgramFocus::Hypertrophy
                }
            }
        }
    }

    pub fn validate(&self) -> Result<(), DomainError> {
        match self {
            Self::Bodyweight {
                baseline_kg,
                target_kg,
            } => {
                check("baseline", *baseline_kg, WEIGHT_BOUNDS)?;
                check("target", *target_kg, WEIGHT_BOUNDS)?;
                if (baseline_kg - target_kg).abs() < 0.1 {
                    return Err(DomainError::Invalid(
                        "the target must differ from the baseline".into(),
                    ));
                }
            }
            Self::ExerciseEst1Rm {
                baseline_kg,
                target_kg,
                ..
            } => {
                check("baseline", *baseline_kg, LIFT_BOUNDS)?;
                check("target", *target_kg, LIFT_BOUNDS)?;
                if *target_kg <= *baseline_kg {
                    // Nobody sets a goal to lift less; a deload is programming,
                    // not a goal.
                    return Err(DomainError::Invalid(
                        "a lift target must be above its baseline".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Where the goal stands. Evidence-carrying, like every status here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GoalStatus {
    Active,
    Achieved {
        achieved_at: DateTime<Utc>,
        /// Who confirmed it — the athlete or their coach. Confirmation is a
        /// human act on purpose: the data says "target crossed", a person says
        /// "done", and the difference matters on a noisy series.
        confirmed_by: UserId,
    },
    Abandoned {
        abandoned_at: DateTime<Utc>,
        abandoned_by: UserId,
    },
}

impl GoalStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Achieved { .. } => "achieved",
            Self::Abandoned { .. } => "abandoned",
        }
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct Goal {
    pub id: GoalId,
    pub gym_id: GymId,
    pub athlete_id: UserId,
    /// Often the athlete themselves; goals are the one coaching artefact a
    /// member may create for themselves.
    pub set_by: UserId,
    pub metric: GoalMetric,
    #[schema(nullable)]
    pub target_date: Option<NaiveDate>,
    pub status: GoalStatus,
    pub created_at: DateTime<Utc>,
}

impl Goal {
    pub fn new(
        gym_id: GymId,
        athlete_id: UserId,
        set_by: UserId,
        metric: GoalMetric,
        target_date: Option<NaiveDate>,
        today: NaiveDate,
        now: DateTime<Utc>,
    ) -> Result<Self, DomainError> {
        metric.validate()?;

        if let Some(date) = target_date {
            if date <= today {
                return Err(DomainError::Invalid(
                    "a target date must be in the future".into(),
                ));
            }
            if (date - today).num_days() > MAX_TARGET_HORIZON_DAYS {
                return Err(DomainError::Invalid(
                    "a target date more than two years out is a dream, not a goal".into(),
                ));
            }
        }

        Ok(Self {
            id: GoalId::new(),
            gym_id,
            athlete_id,
            set_by,
            metric,
            target_date,
            status: GoalStatus::Active,
            created_at: now,
        })
    }

    pub fn achieve(&mut self, confirmed_by: UserId, now: DateTime<Utc>) -> Result<(), DomainError> {
        self.ensure_active("confirm")?;
        self.status = GoalStatus::Achieved {
            achieved_at: now,
            confirmed_by,
        };
        Ok(())
    }

    pub fn abandon(&mut self, abandoned_by: UserId, now: DateTime<Utc>) -> Result<(), DomainError> {
        self.ensure_active("abandon")?;
        self.status = GoalStatus::Abandoned {
            abandoned_at: now,
            abandoned_by,
        };
        Ok(())
    }

    fn ensure_active(&self, action: &str) -> Result<(), DomainError> {
        if self.status.is_active() {
            return Ok(());
        }
        Err(DomainError::Invalid(format!(
            "cannot {action} a goal that is already {}",
            self.status.as_str()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 7, 18).unwrap()
    }

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn cut() -> GoalMetric {
        GoalMetric::Bodyweight {
            baseline_kg: 82.0,
            target_kg: 78.0,
        }
    }

    fn make(metric: GoalMetric, target_date: Option<NaiveDate>) -> Result<Goal, DomainError> {
        Goal::new(
            GymId::new(),
            UserId::new(),
            UserId::new(),
            metric,
            target_date,
            today(),
            now(),
        )
    }

    #[test]
    fn a_cut_and_a_gain_are_both_valid_bodyweight_goals() {
        assert!(make(cut(), None).is_ok());
        assert!(
            make(
                GoalMetric::Bodyweight {
                    baseline_kg: 70.0,
                    target_kg: 75.0
                },
                None
            )
            .is_ok()
        );
    }

    #[test]
    fn a_target_equal_to_the_baseline_is_not_a_goal() {
        assert!(
            make(
                GoalMetric::Bodyweight {
                    baseline_kg: 80.0,
                    target_kg: 80.0
                },
                None
            )
            .is_err()
        );
    }

    #[test]
    fn a_lift_target_below_its_baseline_is_refused() {
        // A deload is programming, not a goal.
        assert!(
            make(
                GoalMetric::ExerciseEst1Rm {
                    exercise_id: ExerciseId::new(),
                    baseline_kg: 100.0,
                    target_kg: 90.0,
                },
                None
            )
            .is_err()
        );
    }

    #[test]
    fn bounds_catch_slipped_digits() {
        assert!(
            make(
                GoalMetric::Bodyweight {
                    baseline_kg: 82.0,
                    target_kg: 780.0
                },
                None
            )
            .is_err()
        );
        assert!(
            make(
                GoalMetric::ExerciseEst1Rm {
                    exercise_id: ExerciseId::new(),
                    baseline_kg: 84.0,
                    target_kg: 9999.0,
                },
                None
            )
            .is_err()
        );
    }

    #[test]
    fn target_dates_must_be_future_but_not_fantasy() {
        assert!(
            make(cut(), Some(today())).is_err(),
            "today is not a deadline"
        );
        assert!(
            make(cut(), Some(today() + chrono::Duration::days(90))).is_ok(),
            "a season out is normal"
        );
        assert!(
            make(cut(), Some(today() + chrono::Duration::days(1000))).is_err(),
            "two years is the horizon"
        );
    }

    #[test]
    fn achieving_records_the_confirmer_and_closes_the_goal() {
        let mut goal = make(cut(), None).unwrap();
        let coach = UserId::new();
        goal.achieve(coach, now()).unwrap();

        match goal.status {
            GoalStatus::Achieved { confirmed_by, .. } => assert_eq!(confirmed_by, coach),
            ref other => panic!("expected achieved, got {other:?}"),
        }
        assert!(
            goal.abandon(UserId::new(), now()).is_err(),
            "closed is closed"
        );
    }

    #[test]
    fn abandoning_is_final_too() {
        let mut goal = make(cut(), None).unwrap();
        goal.abandon(UserId::new(), now()).unwrap();
        assert!(goal.achieve(UserId::new(), now()).is_err());
    }

    #[test]
    fn metric_round_trips_through_json() {
        let goal = make(cut(), None).unwrap();
        let json = serde_json::to_string(&goal.metric).unwrap();
        assert!(json.contains("\"kind\":\"bodyweight\""), "got {json}");
        assert_eq!(
            serde_json::from_str::<GoalMetric>(&json).unwrap(),
            goal.metric
        );

        // The tag a human would write, not serde's digit-mangled guess.
        let lift = GoalMetric::ExerciseEst1Rm {
            exercise_id: ExerciseId::new(),
            baseline_kg: 85.0,
            target_kg: 100.0,
        };
        let json = serde_json::to_string(&lift).unwrap();
        assert!(json.contains("\"kind\":\"exercise_est_1rm\""), "got {json}");
    }

    #[test]
    fn goals_map_to_the_focus_a_coach_would_pick() {
        use crate::program::ProgramFocus;

        assert_eq!(cut().recommended_focus(), ProgramFocus::Conditioning);
        assert_eq!(
            GoalMetric::Bodyweight {
                baseline_kg: 70.0,
                target_kg: 76.0
            }
            .recommended_focus(),
            ProgramFocus::Hypertrophy
        );
        assert_eq!(
            GoalMetric::ExerciseEst1Rm {
                exercise_id: ExerciseId::new(),
                baseline_kg: 70.0,
                target_kg: 100.0,
            }
            .recommended_focus(),
            ProgramFocus::Strength
        );
    }

    #[test]
    fn specialty_matching_is_dumb_on_purpose() {
        use crate::program::ProgramFocus;

        assert!(ProgramFocus::Strength.matches_specialty("Powerlifting"));
        assert!(ProgramFocus::Strength.matches_specialty("  barbell strength  "));
        assert!(ProgramFocus::Conditioning.matches_specialty("Weight loss coaching"));
        assert!(ProgramFocus::Hypertrophy.matches_specialty("Bodybuilding prep"));

        // No fuzzy cleverness: an unrelated specialty simply does not match.
        assert!(!ProgramFocus::Strength.matches_specialty("Prenatal yoga"));
        assert!(!ProgramFocus::Conditioning.matches_specialty("Powerlifting"));
    }

    #[test]
    fn deserialised_metrics_are_still_validated() {
        // Same serde hole as prescriptions: shapes parse, values must re-check.
        let absurd: GoalMetric =
            serde_json::from_str(r#"{"kind":"bodyweight","baseline_kg":82,"target_kg":9000}"#)
                .unwrap();
        assert!(absurd.validate().is_err());
    }
}

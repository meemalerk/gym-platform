//! The content inside a programme version: weeks → workouts → prescribed exercises.
//!
//! ```text
//! ProgramVersion
//!   └── ProgramWeek        (week 1, week 2, …)
//!         └── WorkoutTemplate   ("Upper A", day 1)
//!               └── TemplateExercise  (back squat → 4 × 6-8 @ RIR 2)
//! ```
//!
//! Everything here is content *of a version*, never of a programme. That is what
//! lets a published version stay frozen while a new draft is edited beside it
//! (ADR-0006) — the rows are simply owned by different versions.
//!
//! Ordering is explicit (`position`) rather than implied by insertion order or
//! id. A coach reorders exercises within a workout constantly, and "whatever
//! order the database returned" is not a plan.

use serde::{Deserialize, Serialize};

use crate::{
    DomainError,
    exercise::Modality,
    ids::{ExerciseId, ProgramVersionId, ProgramWeekId, TemplateExerciseId, WorkoutTemplateId},
    prescription::ExercisePrescription,
    validated_name,
};

/// Sanity bounds. A year of programming is already an unusually long block.
const MAX_WEEK_NUMBER: i32 = 52;
/// Seven named days plus room for doubles.
const MAX_DAY_NUMBER: i32 = 14;

/// One week of a version's plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ProgramWeek {
    pub id: ProgramWeekId,
    pub version_id: ProgramVersionId,
    /// 1-based. Week 0 is not a thing a coach says.
    pub week_number: i32,
    #[schema(nullable)]
    pub label: Option<String>,
}

impl ProgramWeek {
    pub fn new(
        version_id: ProgramVersionId,
        week_number: i32,
        label: Option<&str>,
    ) -> Result<Self, DomainError> {
        if !(1..=MAX_WEEK_NUMBER).contains(&week_number) {
            return Err(DomainError::Invalid(format!(
                "week number must be between 1 and {MAX_WEEK_NUMBER}"
            )));
        }
        let label = match label.map(str::trim) {
            None | Some("") => None,
            Some(text) => Some(validated_name("label", text, 120)?),
        };

        Ok(Self {
            id: ProgramWeekId::new(),
            version_id,
            week_number,
            label,
        })
    }
}

/// One session within a week — "Upper A", "Day 3".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WorkoutTemplate {
    pub id: WorkoutTemplateId,
    pub week_id: ProgramWeekId,
    /// Which day within the week. 1-based.
    pub day_number: i32,
    pub name: String,
    #[schema(nullable)]
    pub notes: Option<String>,
}

impl WorkoutTemplate {
    pub fn new(
        week_id: ProgramWeekId,
        day_number: i32,
        name: &str,
        notes: Option<&str>,
    ) -> Result<Self, DomainError> {
        if !(1..=MAX_DAY_NUMBER).contains(&day_number) {
            return Err(DomainError::Invalid(format!(
                "day number must be between 1 and {MAX_DAY_NUMBER}"
            )));
        }
        let notes = match notes.map(str::trim) {
            None | Some("") => None,
            Some(text) => Some(validated_name("notes", text, 2000)?),
        };

        Ok(Self {
            id: WorkoutTemplateId::new(),
            week_id,
            day_number,
            name: validated_name("name", name, 120)?,
            notes,
        })
    }
}

/// One prescribed exercise inside a workout.
///
/// Holds the prescription *and* a reference to the catalogue exercise. The
/// prescription is copied in rather than looked up live, because the catalogue is
/// mutable and a published plan is not — renaming an exercise must not silently
/// change what a published version prescribed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TemplateExercise {
    pub id: TemplateExerciseId,
    pub workout_id: WorkoutTemplateId,
    pub exercise_id: ExerciseId,
    /// Order within the workout. 1-based, contiguous after any reorder.
    pub position: i32,
    pub prescription: ExercisePrescription,
    #[schema(nullable)]
    pub notes: Option<String>,
}

impl TemplateExercise {
    /// Prescribe an exercise, checking the prescription suits how it is measured.
    ///
    /// `exercise_modality` comes from the catalogue entry. Passing it explicitly
    /// keeps this module pure while still making the mismatch impossible to
    /// persist — the check cannot be forgotten because it is the only constructor.
    pub fn new(
        workout_id: WorkoutTemplateId,
        exercise_id: ExerciseId,
        exercise_modality: &Modality,
        position: i32,
        prescription: ExercisePrescription,
        notes: Option<&str>,
    ) -> Result<Self, DomainError> {
        if position < 1 {
            return Err(DomainError::Invalid("position must be 1 or more".into()));
        }
        // Deserialised prescriptions never went through a validating constructor,
        // so re-check the bounds here. This is the only path to persistence,
        // which is what makes it sufficient.
        prescription.validate()?;
        prescription.check_matches(exercise_modality)?;

        let notes = match notes.map(str::trim) {
            None | Some("") => None,
            Some(text) => Some(validated_name("notes", text, 1000)?),
        };

        Ok(Self {
            id: TemplateExerciseId::new(),
            workout_id,
            exercise_id,
            position,
            prescription,
            notes,
        })
    }
}

/// Renumber to 1..n, preserving the given order.
///
/// Reordering by editing one row's position is how lists end up with two items
/// at position 3 and nothing at 5. The caller supplies the desired order; this
/// makes the numbering contiguous again.
pub fn renumber(items: &mut [TemplateExercise]) {
    for (index, item) in items.iter_mut().enumerate() {
        item.position = i32::try_from(index + 1).unwrap_or(i32::MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prescription::RepRange;

    fn version() -> ProgramVersionId {
        ProgramVersionId::new()
    }

    fn workout() -> WorkoutTemplateId {
        WorkoutTemplateId::new()
    }

    fn reps() -> ExercisePrescription {
        ExercisePrescription::repetitions(4, RepRange::new(6, 8).unwrap(), Some(2)).unwrap()
    }

    #[test]
    fn creates_a_week() {
        let w = ProgramWeek::new(version(), 1, Some("  Accumulation ")).unwrap();
        assert_eq!(w.week_number, 1);
        assert_eq!(w.label.as_deref(), Some("Accumulation"));
    }

    #[test]
    fn rejects_week_zero_and_absurd_weeks() {
        assert!(ProgramWeek::new(version(), 0, None).is_err());
        assert!(ProgramWeek::new(version(), -1, None).is_err());
        assert!(ProgramWeek::new(version(), 53, None).is_err());
        assert!(ProgramWeek::new(version(), 52, None).is_ok());
    }

    #[test]
    fn blank_label_normalises_to_none() {
        let w = ProgramWeek::new(version(), 2, Some("   ")).unwrap();
        assert!(w.label.is_none());
    }

    #[test]
    fn creates_a_workout() {
        let w = WorkoutTemplate::new(ProgramWeekId::new(), 1, " Upper A ", None).unwrap();
        assert_eq!(w.name, "Upper A");
    }

    #[test]
    fn rejects_day_zero_and_unnamed_workouts() {
        assert!(WorkoutTemplate::new(ProgramWeekId::new(), 0, "Upper", None).is_err());
        assert!(WorkoutTemplate::new(ProgramWeekId::new(), 1, "   ", None).is_err());
    }

    #[test]
    fn prescribes_an_exercise() {
        let t = TemplateExercise::new(
            workout(),
            ExerciseId::new(),
            &Modality::Repetitions,
            1,
            reps(),
            None,
        )
        .unwrap();
        assert_eq!(t.position, 1);
    }

    #[test]
    fn a_mismatched_prescription_cannot_be_persisted() {
        // The constructor is the only way to build one, so this check cannot be
        // skipped by a caller who forgot.
        let err = TemplateExercise::new(
            workout(),
            ExerciseId::new(),
            &Modality::Distance,
            1,
            reps(),
            None,
        );
        assert!(err.is_err(), "reps prescribed for a distance exercise");
    }

    #[test]
    fn rejects_zero_position() {
        assert!(
            TemplateExercise::new(
                workout(),
                ExerciseId::new(),
                &Modality::Repetitions,
                0,
                reps(),
                None
            )
            .is_err()
        );
    }

    #[test]
    fn renumbering_closes_gaps_and_duplicates() {
        let w = workout();
        let mut items: Vec<_> = [5, 5, 9]
            .into_iter()
            .map(|position| {
                TemplateExercise::new(
                    w,
                    ExerciseId::new(),
                    &Modality::Repetitions,
                    position,
                    reps(),
                    None,
                )
                .unwrap()
            })
            .collect();

        renumber(&mut items);

        assert_eq!(
            items.iter().map(|i| i.position).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn renumbering_preserves_order() {
        let w = workout();
        let mut items: Vec<_> = (1..=3)
            .map(|position| {
                TemplateExercise::new(
                    w,
                    ExerciseId::new(),
                    &Modality::Repetitions,
                    position,
                    reps(),
                    None,
                )
                .unwrap()
            })
            .collect();
        let ids: Vec<_> = items.iter().map(|i| i.id).collect();

        renumber(&mut items);

        assert_eq!(items.iter().map(|i| i.id).collect::<Vec<_>>(), ids);
    }
}

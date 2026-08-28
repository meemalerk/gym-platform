//! What a member is told to do for one exercise in one workout.
//!
//! Modelled as an enum keyed on how the exercise is measured, so the impossible
//! combinations simply cannot be written down: there is no way to express "4 sets
//! of 8 reps over 5 kilometres", because no variant has both fields
//! (docs/domain-model.md).
//!
//! Every constructor validates. The bounds are not arbitrary — they are the range
//! a human can actually perform, and they exist to catch the fat-finger entry (300
//! sets, a 40-hour plank) that would otherwise reach a member's phone as gospel.

use serde::{Deserialize, Serialize};

use crate::{DomainError, exercise::Modality};

/// Upper bounds. Generous enough never to obstruct real programming, tight
/// enough that a typo is caught at the boundary rather than shipped.
const MAX_SETS: u8 = 20;
const MAX_REPS: u16 = 500;
const MAX_SECONDS: u32 = 36_000; // 10 hours
const MAX_METRES: u32 = 1_000_000; // 1000 km
const MAX_RIR: u8 = 10;
/// 5 s/km is faster than any human; 2 hours/km is slower than walking.
const MIN_PACE_SECONDS_PER_KM: u32 = 5;
const MAX_PACE_SECONDS_PER_KM: u32 = 7_200;

/// A target repetition range, e.g. 6–8. `min == max` expresses a fixed target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct RepRange {
    pub min: u16,
    pub max: u16,
}

impl RepRange {
    pub fn new(min: u16, max: u16) -> Result<Self, DomainError> {
        if min == 0 {
            return Err(DomainError::Invalid(
                "rep range must start at 1 or more".into(),
            ));
        }
        if max > MAX_REPS {
            return Err(DomainError::Invalid(format!(
                "rep range must not exceed {MAX_REPS} repetitions"
            )));
        }
        if min > max {
            // Caught here rather than "helpfully" swapped: a coach who typed 8–6
            // meant something, and silently reinterpreting it is how a member
            // ends up doing the wrong work.
            return Err(DomainError::Invalid(format!(
                "rep range minimum ({min}) must not exceed its maximum ({max})"
            )));
        }
        Ok(Self { min, max })
    }

    /// A single fixed target, e.g. exactly 5.
    pub fn exactly(reps: u16) -> Result<Self, DomainError> {
        Self::new(reps, reps)
    }

    #[must_use]
    pub const fn is_fixed(&self) -> bool {
        self.min == self.max
    }
}

/// A target pace band in seconds per kilometre. Lower is faster, so `min` is the
/// *quickest* acceptable pace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PaceRange {
    pub min_seconds_per_km: u32,
    pub max_seconds_per_km: u32,
}

impl PaceRange {
    pub fn new(min_seconds_per_km: u32, max_seconds_per_km: u32) -> Result<Self, DomainError> {
        if min_seconds_per_km < MIN_PACE_SECONDS_PER_KM
            || max_seconds_per_km > MAX_PACE_SECONDS_PER_KM
        {
            return Err(DomainError::Invalid(format!(
                "pace must be between {MIN_PACE_SECONDS_PER_KM} and {MAX_PACE_SECONDS_PER_KM} seconds per kilometre"
            )));
        }
        if min_seconds_per_km > max_seconds_per_km {
            return Err(DomainError::Invalid(
                "pace range minimum must not be slower than its maximum".into(),
            ));
        }
        Ok(Self {
            min_seconds_per_km,
            max_seconds_per_km,
        })
    }
}

/// The prescription for one exercise, shaped by how that exercise is measured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExercisePrescription {
    Repetitions {
        sets: u8,
        target: RepRange,
        /// Reps in reserve — how many more the member should have left. `None`
        /// means the coach did not specify an intensity, which is different from
        /// specifying zero (train to failure).
        #[schema(nullable)]
        rir: Option<u8>,
    },
    Duration {
        sets: u8,
        seconds: u32,
    },
    Distance {
        metres: u32,
        #[schema(nullable)]
        pace: Option<PaceRange>,
    },
}

impl ExercisePrescription {
    pub fn repetitions(sets: u8, target: RepRange, rir: Option<u8>) -> Result<Self, DomainError> {
        validate_sets(sets)?;
        if let Some(rir) = rir
            && rir > MAX_RIR
        {
            return Err(DomainError::Invalid(format!(
                "reps in reserve must be at most {MAX_RIR}"
            )));
        }
        Ok(Self::Repetitions { sets, target, rir })
    }

    pub fn duration(sets: u8, seconds: u32) -> Result<Self, DomainError> {
        validate_sets(sets)?;
        if seconds == 0 || seconds > MAX_SECONDS {
            return Err(DomainError::Invalid(format!(
                "duration must be between 1 and {MAX_SECONDS} seconds"
            )));
        }
        Ok(Self::Duration { sets, seconds })
    }

    pub fn distance(metres: u32, pace: Option<PaceRange>) -> Result<Self, DomainError> {
        if metres == 0 || metres > MAX_METRES {
            return Err(DomainError::Invalid(format!(
                "distance must be between 1 and {MAX_METRES} metres"
            )));
        }
        Ok(Self::Distance { metres, pace })
    }

    /// Re-check every bound on an already-constructed value.
    ///
    /// The constructors above validate, but `Deserialize` does not go through
    /// them — a prescription arriving as JSON from an HTTP request is built
    /// field by field, so "99 sets" deserialises perfectly happily. This is the
    /// backstop, called by `TemplateExercise::new`, which is the only path to
    /// persistence. Found by scripts/verify-programs.sh: the API accepted 99
    /// sets and stored it.
    pub fn validate(&self) -> Result<(), DomainError> {
        match self {
            Self::Repetitions { sets, target, rir } => {
                validate_sets(*sets)?;
                // Rebuilding the range re-applies its invariants (min <= max,
                // non-zero, within bounds) without duplicating them here.
                RepRange::new(target.min, target.max)?;
                if let Some(rir) = rir
                    && *rir > MAX_RIR
                {
                    return Err(DomainError::Invalid(format!(
                        "reps in reserve must be at most {MAX_RIR}"
                    )));
                }
            }
            Self::Duration { sets, seconds } => {
                validate_sets(*sets)?;
                if *seconds == 0 || *seconds > MAX_SECONDS {
                    return Err(DomainError::Invalid(format!(
                        "duration must be between 1 and {MAX_SECONDS} seconds"
                    )));
                }
            }
            Self::Distance { metres, pace } => {
                if *metres == 0 || *metres > MAX_METRES {
                    return Err(DomainError::Invalid(format!(
                        "distance must be between 1 and {MAX_METRES} metres"
                    )));
                }
                if let Some(pace) = pace {
                    PaceRange::new(pace.min_seconds_per_km, pace.max_seconds_per_km)?;
                }
            }
        }
        Ok(())
    }

    /// Which measurement this prescription speaks in.
    #[must_use]
    pub const fn modality(&self) -> Modality {
        match self {
            Self::Repetitions { .. } => Modality::Repetitions,
            Self::Duration { .. } => Modality::Duration,
            Self::Distance { .. } => Modality::Distance,
        }
    }

    /// Does this prescription make sense for that exercise?
    ///
    /// Prescribing "4 sets of 8 reps" for an exercise measured in distance is
    /// nonsense the type system cannot catch on its own, because both are valid
    /// values independently — only their *pairing* is wrong.
    pub fn check_matches(&self, exercise: &Modality) -> Result<(), DomainError> {
        let prescribed = self.modality();
        if &prescribed == exercise {
            return Ok(());
        }
        Err(DomainError::Invalid(format!(
            "a {} prescription cannot be used for an exercise measured in {}",
            prescribed.as_str(),
            exercise.as_str()
        )))
    }
}

fn validate_sets(sets: u8) -> Result<(), DomainError> {
    if sets == 0 || sets > MAX_SETS {
        return Err(DomainError::Invalid(format!(
            "sets must be between 1 and {MAX_SETS}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_normal_strength_prescription() {
        let p =
            ExercisePrescription::repetitions(4, RepRange::new(6, 8).unwrap(), Some(2)).unwrap();
        assert_eq!(p.modality(), Modality::Repetitions);
    }

    #[test]
    fn rejects_inverted_rep_range_rather_than_swapping_it() {
        // 8-6 is a typo with an intent behind it. Guessing is worse than refusing.
        assert!(RepRange::new(8, 6).is_err());
    }

    #[test]
    fn allows_a_fixed_target() {
        let r = RepRange::exactly(5).unwrap();
        assert!(r.is_fixed());
    }

    #[test]
    fn rejects_zero_reps() {
        assert!(RepRange::new(0, 5).is_err());
    }

    #[test]
    fn rejects_absurd_set_counts() {
        assert!(ExercisePrescription::repetitions(0, RepRange::exactly(5).unwrap(), None).is_err());
        assert!(
            ExercisePrescription::repetitions(21, RepRange::exactly(5).unwrap(), None).is_err()
        );
    }

    #[test]
    fn zero_rir_is_meaningful_and_distinct_from_none() {
        // Zero reps in reserve means "to failure" — a real instruction, not a
        // missing value. If these collapsed, the app would lose the distinction.
        let to_failure =
            ExercisePrescription::repetitions(3, RepRange::exactly(8).unwrap(), Some(0)).unwrap();
        let unspecified =
            ExercisePrescription::repetitions(3, RepRange::exactly(8).unwrap(), None).unwrap();
        assert_ne!(to_failure, unspecified);
    }

    #[test]
    fn rejects_impossible_rir() {
        assert!(
            ExercisePrescription::repetitions(3, RepRange::exactly(8).unwrap(), Some(11)).is_err()
        );
    }

    #[test]
    fn rejects_zero_and_absurd_durations() {
        assert!(ExercisePrescription::duration(3, 0).is_err());
        assert!(ExercisePrescription::duration(3, MAX_SECONDS + 1).is_err());
        assert!(ExercisePrescription::duration(3, 60).is_ok());
    }

    #[test]
    fn rejects_zero_distance() {
        assert!(ExercisePrescription::distance(0, None).is_err());
        assert!(ExercisePrescription::distance(5_000, None).is_ok());
    }

    #[test]
    fn rejects_inverted_pace_range() {
        assert!(PaceRange::new(400, 300).is_err());
        assert!(PaceRange::new(300, 400).is_ok());
    }

    #[test]
    fn rejects_superhuman_pace() {
        assert!(PaceRange::new(1, 300).is_err());
    }

    #[test]
    fn prescription_must_match_the_exercise_it_is_attached_to() {
        let reps =
            ExercisePrescription::repetitions(4, RepRange::new(6, 8).unwrap(), None).unwrap();

        assert!(reps.check_matches(&Modality::Repetitions).is_ok());
        // The whole point: valid prescription, valid exercise, invalid pairing.
        assert!(reps.check_matches(&Modality::Distance).is_err());
        assert!(reps.check_matches(&Modality::Duration).is_err());
    }

    #[test]
    fn deserialised_prescriptions_are_still_validated() {
        // The hole this closes: serde builds the value field by field, so the
        // validating constructors never run. The API accepted 99 sets and stored
        // it until `validate` was added.
        let absurd: ExercisePrescription =
            serde_json::from_str(r#"{"kind":"repetitions","sets":99,"target":{"min":6,"max":8}}"#)
                .expect("shape is valid even though the values are not");

        assert!(absurd.validate().is_err(), "99 sets must be refused");
    }

    #[test]
    fn validate_catches_every_out_of_range_field() {
        let cases = [
            r#"{"kind":"repetitions","sets":0,"target":{"min":6,"max":8}}"#,
            r#"{"kind":"repetitions","sets":4,"target":{"min":8,"max":6}}"#,
            r#"{"kind":"repetitions","sets":4,"target":{"min":0,"max":8}}"#,
            r#"{"kind":"repetitions","sets":4,"target":{"min":6,"max":8},"rir":50}"#,
            r#"{"kind":"duration","sets":4,"seconds":0}"#,
            r#"{"kind":"duration","sets":4,"seconds":999999}"#,
            r#"{"kind":"distance","metres":0}"#,
            r#"{"kind":"distance","metres":5000,"pace":{"min_seconds_per_km":400,"max_seconds_per_km":300}}"#,
        ];

        for case in cases {
            let p: ExercisePrescription = serde_json::from_str(case).unwrap();
            assert!(p.validate().is_err(), "should have been refused: {case}");
        }
    }

    #[test]
    fn validate_accepts_realistic_prescriptions() {
        let cases = [
            r#"{"kind":"repetitions","sets":4,"target":{"min":6,"max":8},"rir":2}"#,
            r#"{"kind":"repetitions","sets":1,"target":{"min":1,"max":1},"rir":0}"#,
            r#"{"kind":"duration","sets":3,"seconds":45}"#,
            r#"{"kind":"distance","metres":5000}"#,
            r#"{"kind":"distance","metres":2000,"pace":{"min_seconds_per_km":300,"max_seconds_per_km":400}}"#,
        ];

        for case in cases {
            let p: ExercisePrescription = serde_json::from_str(case).unwrap();
            assert!(p.validate().is_ok(), "should have been accepted: {case}");
        }
    }

    #[test]
    fn serialises_with_a_kind_tag() {
        let p = ExercisePrescription::duration(3, 45).unwrap();
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"kind\":\"duration\""), "got {json}");

        let back: ExercisePrescription = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }
}

//! Body measurements — person-owned wellness data, one row per person per day.
//!
//! Everything here is self-reported and about a body, which drives three
//! choices: the person may delete their own entries (it is their data, not an
//! accountability record); height lives on the profile because it is
//! semi-static; and BMI is never stored — it is weight ÷ height², computed at
//! the edge, because a stored derivative of two editable numbers is a number
//! that lies eventually.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{DomainError, ids::UserId, validated_name};

/// Plausibility bounds. Wide enough for every real human; tight enough that a
/// slipped digit (785 kg, 20 cm waist on an adult) is caught at entry.
const WEIGHT_KG: (f64, f64) = (20.0, 500.0);
const BODY_FAT_PERCENT: (f64, f64) = (1.0, 75.0);
const GIRTH_CM: (f64, f64) = (10.0, 300.0);
const ARM_CM: (f64, f64) = (5.0, 100.0);
const THIGH_CM: (f64, f64) = (10.0, 150.0);
/// Backfilling old logbooks is legitimate; a decade is the credulity limit.
const MAX_BACKDATE_DAYS: i64 = 3653;

fn bounded(
    field: &'static str,
    value: Option<f64>,
    (min, max): (f64, f64),
) -> Result<Option<f64>, DomainError> {
    match value {
        None => Ok(None),
        Some(v) if v.is_finite() && (min..=max).contains(&v) => Ok(Some(v)),
        Some(_) => Err(DomainError::Invalid(format!(
            "{field} must be between {min} and {max}"
        ))),
    }
}

/// One day's numbers. Every field optional — you weigh in most days and tape
/// your waist monthly — but a row with nothing at all in it is refused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct BodyMeasurement {
    pub user_id: UserId,
    pub measured_on: NaiveDate,
    #[schema(nullable)]
    pub weight_kg: Option<f64>,
    #[schema(nullable)]
    pub body_fat_percent: Option<f64>,
    #[schema(nullable)]
    pub waist_cm: Option<f64>,
    #[schema(nullable)]
    pub hip_cm: Option<f64>,
    #[schema(nullable)]
    pub chest_cm: Option<f64>,
    #[schema(nullable)]
    pub arm_cm: Option<f64>,
    #[schema(nullable)]
    pub thigh_cm: Option<f64>,
    #[schema(nullable)]
    pub notes: Option<String>,
}

/// The raw numbers as entered, before validation.
#[derive(Debug, Clone, Default)]
pub struct MeasurementEntry {
    pub weight_kg: Option<f64>,
    pub body_fat_percent: Option<f64>,
    pub waist_cm: Option<f64>,
    pub hip_cm: Option<f64>,
    pub chest_cm: Option<f64>,
    pub arm_cm: Option<f64>,
    pub thigh_cm: Option<f64>,
    pub notes: Option<String>,
}

impl BodyMeasurement {
    pub fn new(
        user_id: UserId,
        measured_on: NaiveDate,
        entry: MeasurementEntry,
        today: NaiveDate,
    ) -> Result<Self, DomainError> {
        // `today` arrives as the SERVER's date, which is UTC — but "today" for a
        // morning weigh-in is the member's calendar. A member in UTC+5 is already
        // living tomorrow by UTC's clock every evening, and UTC+14 exists. One
        // day of tolerance covers every real timezone; two days is a typo.
        // (Found live: a WSL clock at UTC+5 had every same-day entry refused as
        // "the future".)
        if measured_on > today + chrono::Duration::days(1) {
            return Err(DomainError::Invalid(
                "a measurement cannot be dated in the future".into(),
            ));
        }
        if (today - measured_on).num_days() > MAX_BACKDATE_DAYS {
            return Err(DomainError::Invalid(
                "a measurement cannot be more than ten years old".into(),
            ));
        }

        let notes = match entry.notes.as_deref().map(str::trim) {
            None | Some("") => None,
            Some(text) => Some(validated_name("notes", text, 500)?),
        };

        let measurement = Self {
            user_id,
            measured_on,
            weight_kg: bounded("weight", entry.weight_kg, WEIGHT_KG)?,
            body_fat_percent: bounded("body fat", entry.body_fat_percent, BODY_FAT_PERCENT)?,
            waist_cm: bounded("waist", entry.waist_cm, GIRTH_CM)?,
            hip_cm: bounded("hip", entry.hip_cm, GIRTH_CM)?,
            chest_cm: bounded("chest", entry.chest_cm, GIRTH_CM)?,
            arm_cm: bounded("arm", entry.arm_cm, ARM_CM)?,
            thigh_cm: bounded("thigh", entry.thigh_cm, THIGH_CM)?,
            notes,
        };

        if measurement.weight_kg.is_none()
            && measurement.body_fat_percent.is_none()
            && measurement.waist_cm.is_none()
            && measurement.hip_cm.is_none()
            && measurement.chest_cm.is_none()
            && measurement.arm_cm.is_none()
            && measurement.thigh_cm.is_none()
        {
            // Notes alone are a diary entry, not a measurement.
            return Err(DomainError::Invalid(
                "a measurement needs at least one number".into(),
            ));
        }

        Ok(measurement)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 7, 18).unwrap()
    }

    fn weigh(kg: f64) -> MeasurementEntry {
        MeasurementEntry {
            weight_kg: Some(kg),
            ..MeasurementEntry::default()
        }
    }

    #[test]
    fn a_morning_weigh_in_is_enough() {
        let m = BodyMeasurement::new(UserId::new(), today(), weigh(81.4), today()).unwrap();
        assert_eq!(m.weight_kg, Some(81.4));
    }

    #[test]
    fn an_empty_row_is_not_a_measurement() {
        let err = BodyMeasurement::new(
            UserId::new(),
            today(),
            MeasurementEntry {
                notes: Some("felt great".into()),
                ..MeasurementEntry::default()
            },
            today(),
        );
        assert!(err.is_err(), "notes alone are a diary entry");
    }

    #[test]
    fn refuses_slipped_digits() {
        let attempt =
            |entry: MeasurementEntry| BodyMeasurement::new(UserId::new(), today(), entry, today());

        assert!(attempt(weigh(785.0)).is_err(), "785 kg is a typo");
        assert!(attempt(weigh(8.0)).is_err(), "8 kg is a typo");
        assert!(
            attempt(MeasurementEntry {
                body_fat_percent: Some(95.0),
                ..MeasurementEntry::default()
            })
            .is_err()
        );
        assert!(
            attempt(MeasurementEntry {
                waist_cm: Some(2.0),
                ..MeasurementEntry::default()
            })
            .is_err()
        );
    }

    #[test]
    fn dates_are_bounded_but_backfill_is_welcome() {
        let attempt = |on: NaiveDate| BodyMeasurement::new(UserId::new(), on, weigh(80.0), today());

        assert!(
            attempt(today() - chrono::Duration::days(400)).is_ok(),
            "old logbooks import"
        );
        // One day ahead of the server's UTC date is a member east of Greenwich,
        // not a time traveller. Two days is nobody's timezone.
        assert!(
            attempt(today() + chrono::Duration::days(1)).is_ok(),
            "UTC+X members live tomorrow"
        );
        assert!(
            attempt(today() + chrono::Duration::days(2)).is_err(),
            "the actual future"
        );
        assert!(
            attempt(today() - chrono::Duration::days(4000)).is_err(),
            "a decade is the limit"
        );
    }

    #[test]
    fn nan_and_infinity_are_refused() {
        assert!(BodyMeasurement::new(UserId::new(), today(), weigh(f64::NAN), today()).is_err());
        assert!(
            BodyMeasurement::new(UserId::new(), today(), weigh(f64::INFINITY), today()).is_err()
        );
    }
}

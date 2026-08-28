//! Person-owned profiles (ADR-0014): they follow the account, never the gym.
//!
//! Two profiles, two audiences, deliberately separate:
//!
//! - **Athlete profile** — who you are as a trainee: goals, training age,
//!   limitations. Written by you; readable by your coaches, because "watch the
//!   left knee" is exactly what a coach must know before writing week one.
//! - **Trainer profile** — who you are as a coach: headline, bio, credentials.
//!   Written by you; shown wherever you coach.
//!
//! One account may hold both — the trainer who is also somebody's client is the
//! normal case here, not an edge case.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::{DomainError, ids::UserId, validated_name};

const MAX_TEXT: usize = 2000;
const MAX_BIO: usize = 4000;
const MAX_HEADLINE: usize = 120;
const MAX_LIST_ITEMS: usize = 20;
const MAX_LIST_ITEM_LEN: usize = 80;
/// 100 years of training is a typo, not a career.
const MAX_TRAINING_AGE_MONTHS: i32 = 1200;

/// Free text that may be absent: trimmed, bounded, `""` normalised to `None`.
fn optional_text(
    field: &'static str,
    raw: Option<&str>,
    max: usize,
) -> Result<Option<String>, DomainError> {
    match raw.map(str::trim) {
        None | Some("") => Ok(None),
        Some(text) => Ok(Some(validated_name(field, text, max)?)),
    }
}

/// A list of short labels (certifications, specialties): trimmed, de-duplicated
/// case-insensitively, blanks dropped, bounded in count and length.
fn validated_labels(field: &'static str, raw: &[String]) -> Result<Vec<String>, DomainError> {
    let mut seen: Vec<String> = Vec::new();
    let mut out: Vec<String> = Vec::new();

    for item in raw {
        let trimmed = item.trim();
        if trimmed.is_empty() {
            continue;
        }
        let item = validated_name(field, trimmed, MAX_LIST_ITEM_LEN)?;
        let key = item.to_lowercase();
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(item);
    }

    if out.len() > MAX_LIST_ITEMS {
        return Err(DomainError::Invalid(format!(
            "{field} may hold at most {MAX_LIST_ITEMS} entries"
        )));
    }
    Ok(out)
}

/// Who you are as a trainee. All fields optional — an empty profile is a valid
/// profile, not an error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AthleteProfile {
    pub user_id: UserId,
    #[schema(nullable)]
    pub goals: Option<String>,
    /// Months under structured training — the honest unit; "years" rounds away
    /// everyone's first eighteen months.
    #[schema(nullable)]
    pub training_age_months: Option<i32>,
    /// Injuries and constraints a coach must know. Free text on purpose:
    /// medical taxonomies are a liability this platform does not want
    /// (ADR-0016's reasoning applies here too).
    #[schema(nullable)]
    pub limitations: Option<String>,
    #[schema(nullable)]
    pub date_of_birth: Option<NaiveDate>,
    /// Whole centimetres — semi-static, so it lives here rather than on every
    /// measurement row. BMI is computed from this plus the latest weight, never
    /// stored.
    #[schema(nullable)]
    pub height_cm: Option<i32>,
}

impl AthleteProfile {
    pub fn new(
        user_id: UserId,
        goals: Option<&str>,
        training_age_months: Option<i32>,
        limitations: Option<&str>,
        date_of_birth: Option<NaiveDate>,
        height_cm: Option<i32>,
        today: NaiveDate,
    ) -> Result<Self, DomainError> {
        if let Some(months) = training_age_months
            && !(0..=MAX_TRAINING_AGE_MONTHS).contains(&months)
        {
            return Err(DomainError::Invalid(format!(
                "training age must be between 0 and {MAX_TRAINING_AGE_MONTHS} months"
            )));
        }

        if let Some(cm) = height_cm
            && !(50..=280).contains(&cm)
        {
            return Err(DomainError::Invalid(
                "height must be between 50 and 280 cm".into(),
            ));
        }

        if let Some(dob) = date_of_birth {
            if dob >= today {
                return Err(DomainError::Invalid(
                    "date of birth must be in the past".into(),
                ));
            }
            // 13 is the platform floor (store policies and consent law both bite
            // below it); 120 is a keyboard slip, not a member.
            let age_years = (today - dob).num_days() / 366;
            if age_years > 120 {
                return Err(DomainError::Invalid(
                    "date of birth is implausibly old".into(),
                ));
            }
            if age_years < 13 {
                return Err(DomainError::Invalid(
                    "members must be at least 13 years old".into(),
                ));
            }
        }

        Ok(Self {
            user_id,
            goals: optional_text("goals", goals, MAX_TEXT)?,
            training_age_months,
            limitations: optional_text("limitations", limitations, MAX_TEXT)?,
            date_of_birth,
            height_cm,
        })
    }
}

/// Who you are as a coach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TrainerProfile {
    pub user_id: UserId,
    #[schema(nullable)]
    pub headline: Option<String>,
    #[schema(nullable)]
    pub bio: Option<String>,
    pub certifications: Vec<String>,
    pub specialties: Vec<String>,
}

impl TrainerProfile {
    pub fn new(
        user_id: UserId,
        headline: Option<&str>,
        bio: Option<&str>,
        certifications: &[String],
        specialties: &[String],
    ) -> Result<Self, DomainError> {
        Ok(Self {
            user_id,
            headline: optional_text("headline", headline, MAX_HEADLINE)?,
            bio: optional_text("bio", bio, MAX_BIO)?,
            certifications: validated_labels("certifications", certifications)?,
            specialties: validated_labels("specialties", specialties)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 7, 18).unwrap()
    }

    #[test]
    fn an_empty_profile_is_a_valid_profile() {
        let p = AthleteProfile::new(UserId::new(), None, None, None, None, None, today()).unwrap();
        assert!(p.goals.is_none());
    }

    #[test]
    fn blank_text_normalises_to_none() {
        let p = AthleteProfile::new(
            UserId::new(),
            Some("   "),
            None,
            Some(""),
            None,
            None,
            today(),
        )
        .unwrap();
        assert!(p.goals.is_none());
        assert!(p.limitations.is_none());
    }

    #[test]
    fn bounds_training_age() {
        assert!(
            AthleteProfile::new(UserId::new(), None, Some(-1), None, None, None, today()).is_err()
        );
        assert!(
            AthleteProfile::new(UserId::new(), None, Some(1201), None, None, None, today())
                .is_err()
        );
        assert!(
            AthleteProfile::new(UserId::new(), None, Some(18), None, None, None, today()).is_ok()
        );
    }

    #[test]
    fn rejects_impossible_birthdays() {
        let attempt = |dob: NaiveDate| {
            AthleteProfile::new(UserId::new(), None, None, None, Some(dob), None, today())
        };

        assert!(
            attempt(NaiveDate::from_ymd_opt(2030, 1, 1).unwrap()).is_err(),
            "future"
        );
        assert!(
            attempt(NaiveDate::from_ymd_opt(1890, 1, 1).unwrap()).is_err(),
            "too old"
        );
        assert!(
            attempt(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()).is_err(),
            "under 13"
        );
        assert!(attempt(NaiveDate::from_ymd_opt(1990, 5, 20).unwrap()).is_ok());
    }

    #[test]
    fn labels_are_trimmed_deduplicated_and_bounded() {
        let p = TrainerProfile::new(
            UserId::new(),
            None,
            None,
            &[
                "  CSCS ".to_owned(),
                "cscs".to_owned(), // same label, different case: one survives
                String::new(),
                "Precision Nutrition L1".to_owned(),
            ],
            &[],
        )
        .unwrap();

        assert_eq!(p.certifications, vec!["CSCS", "Precision Nutrition L1"]);
    }

    #[test]
    fn too_many_labels_refuse() {
        let many: Vec<String> = (0..21).map(|i| format!("Cert {i}")).collect();
        assert!(TrainerProfile::new(UserId::new(), None, None, &many, &[]).is_err());
    }

    #[test]
    fn overlong_text_refuses() {
        let long = "x".repeat(4001);
        assert!(TrainerProfile::new(UserId::new(), None, Some(&long), &[], &[]).is_err());
    }
}

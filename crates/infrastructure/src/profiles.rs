//! Postgres adapter for person-owned profiles.
//!
//! These tables are outside RLS (migration 0003 documents why), so the queries
//! here are keyed on the user id the SERVICE hands down — never on anything a
//! request could forge. Upserts are full replaces; merge semantics on optional
//! columns are where "I cleared that field" quietly stops working.

use async_trait::async_trait;
use chrono::NaiveDate;
use gym_application::{ApplicationError, ApplicationResult, ports::ProfileRepository};
use gym_domain::{
    UserId,
    measurement::BodyMeasurement,
    profile::{AthleteProfile, TrainerProfile},
};

fn db_err(e: sqlx::Error) -> ApplicationError {
    ApplicationError::internal(e)
}

#[derive(Debug, Clone)]
pub struct PgProfileRepository {
    pool: crate::DbPool,
}

impl PgProfileRepository {
    #[must_use]
    pub const fn new(pool: crate::DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProfileRepository for PgProfileRepository {
    async fn athlete_profile(&self, user: UserId) -> ApplicationResult<Option<AthleteProfile>> {
        let row = sqlx::query!(
            r#"
            SELECT user_id, goals, training_age_months, limitations, date_of_birth, height_cm
            FROM athlete_profiles WHERE user_id = $1
            "#,
            user.into_uuid()
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(row.map(|r| AthleteProfile {
            user_id: UserId::from(r.user_id),
            goals: r.goals,
            training_age_months: r.training_age_months,
            limitations: r.limitations,
            date_of_birth: r.date_of_birth,
            height_cm: r.height_cm,
        }))
    }

    async fn trainer_profile(&self, user: UserId) -> ApplicationResult<Option<TrainerProfile>> {
        let row = sqlx::query!(
            r#"
            SELECT user_id, headline, bio, certifications, specialties
            FROM trainer_profiles WHERE user_id = $1
            "#,
            user.into_uuid()
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(row.map(|r| TrainerProfile {
            user_id: UserId::from(r.user_id),
            headline: r.headline,
            bio: r.bio,
            certifications: r.certifications,
            specialties: r.specialties,
        }))
    }

    async fn upsert_athlete(&self, profile: &AthleteProfile) -> ApplicationResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO athlete_profiles
                (user_id, goals, training_age_months, limitations, date_of_birth, height_cm)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (user_id) DO UPDATE SET
                goals = EXCLUDED.goals,
                training_age_months = EXCLUDED.training_age_months,
                limitations = EXCLUDED.limitations,
                date_of_birth = EXCLUDED.date_of_birth,
                height_cm = EXCLUDED.height_cm,
                updated_at = now()
            "#,
            profile.user_id.into_uuid(),
            profile.goals.as_deref(),
            profile.training_age_months,
            profile.limitations.as_deref(),
            profile.date_of_birth,
            profile.height_cm,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(())
    }

    async fn measurements(&self, user: UserId) -> ApplicationResult<Vec<BodyMeasurement>> {
        let rows = sqlx::query!(
            r#"
            SELECT user_id, measured_on, weight_kg, body_fat_percent,
                   waist_cm, hip_cm, chest_cm, arm_cm, thigh_cm, notes
            FROM body_measurements
            WHERE user_id = $1
            ORDER BY measured_on DESC
            LIMIT 365
            "#,
            user.into_uuid()
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(rows
            .into_iter()
            .map(|r| BodyMeasurement {
                user_id: UserId::from(r.user_id),
                measured_on: r.measured_on,
                weight_kg: r.weight_kg,
                body_fat_percent: r.body_fat_percent,
                waist_cm: r.waist_cm,
                hip_cm: r.hip_cm,
                chest_cm: r.chest_cm,
                arm_cm: r.arm_cm,
                thigh_cm: r.thigh_cm,
                notes: r.notes,
            })
            .collect())
    }

    async fn upsert_measurement(&self, m: &BodyMeasurement) -> ApplicationResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO body_measurements
                (user_id, measured_on, weight_kg, body_fat_percent,
                 waist_cm, hip_cm, chest_cm, arm_cm, thigh_cm, notes)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (user_id, measured_on) DO UPDATE SET
                weight_kg = EXCLUDED.weight_kg,
                body_fat_percent = EXCLUDED.body_fat_percent,
                waist_cm = EXCLUDED.waist_cm,
                hip_cm = EXCLUDED.hip_cm,
                chest_cm = EXCLUDED.chest_cm,
                arm_cm = EXCLUDED.arm_cm,
                thigh_cm = EXCLUDED.thigh_cm,
                notes = EXCLUDED.notes,
                updated_at = now()
            "#,
            m.user_id.into_uuid(),
            m.measured_on,
            m.weight_kg,
            m.body_fat_percent,
            m.waist_cm,
            m.hip_cm,
            m.chest_cm,
            m.arm_cm,
            m.thigh_cm,
            m.notes.as_deref(),
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(())
    }

    async fn delete_measurement(
        &self,
        user: UserId,
        measured_on: NaiveDate,
    ) -> ApplicationResult<bool> {
        let deleted = sqlx::query!(
            "DELETE FROM body_measurements WHERE user_id = $1 AND measured_on = $2",
            user.into_uuid(),
            measured_on,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(deleted.rows_affected() > 0)
    }

    async fn upsert_trainer(&self, profile: &TrainerProfile) -> ApplicationResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO trainer_profiles
                (user_id, headline, bio, certifications, specialties)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (user_id) DO UPDATE SET
                headline = EXCLUDED.headline,
                bio = EXCLUDED.bio,
                certifications = EXCLUDED.certifications,
                specialties = EXCLUDED.specialties,
                updated_at = now()
            "#,
            profile.user_id.into_uuid(),
            profile.headline.as_deref(),
            profile.bio.as_deref(),
            &profile.certifications,
            &profile.specialties,
        )
        .execute(&self.pool)
        .await
        .map_err(db_err)?;

        Ok(())
    }
}

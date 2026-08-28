//! Profile use-cases.
//!
//! Profiles are person-owned and their tables sit OUTSIDE row-level security
//! (a deliberate Phase 1 decision — the auth path needs them before any tenant
//! context exists). That makes this service the only wall: every read of
//! somebody else's profile goes through the coaching gate here, and there is
//! no second line of defence behind it. Treat changes accordingly.

use std::sync::Arc;

use chrono::NaiveDate;
use gym_domain::{
    TenantContext, UserId,
    measurement::{BodyMeasurement, MeasurementEntry},
    profile::{AthleteProfile, TrainerProfile},
    validated_name,
};

use crate::{
    ApplicationError, ApplicationResult,
    ports::{Clock, CoachRepository, ProfileRepository, UserRepository},
};

#[derive(Debug, Clone, Default)]
pub struct UpdateAthleteProfileCommand {
    pub goals: Option<String>,
    pub training_age_months: Option<i32>,
    pub limitations: Option<String>,
    pub date_of_birth: Option<NaiveDate>,
    pub height_cm: Option<i32>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateTrainerProfileCommand {
    pub headline: Option<String>,
    pub bio: Option<String>,
    pub certifications: Vec<String>,
    pub specialties: Vec<String>,
}

#[derive(Clone)]
pub struct ProfileService {
    pub profiles: Arc<dyn ProfileRepository>,
    pub users: Arc<dyn UserRepository>,
    pub relationships: Arc<dyn CoachRepository>,
    pub clock: Arc<dyn Clock>,
}

impl ProfileService {
    /// Both of the caller's profiles. `None` means never filled in — a valid
    /// state the UI should render as an invitation, not an error.
    pub async fn my_profiles(
        &self,
        actor: UserId,
    ) -> ApplicationResult<(Option<AthleteProfile>, Option<TrainerProfile>)> {
        let athlete = self.profiles.athlete_profile(actor).await?;
        let trainer = self.profiles.trainer_profile(actor).await?;
        Ok((athlete, trainer))
    }

    /// Replace the caller's athlete profile. Full replace, not a patch — the
    /// form submits the whole thing, and merge semantics on optional fields are
    /// where "I cleared that box" quietly stops working.
    pub async fn update_athlete(
        &self,
        actor: UserId,
        cmd: UpdateAthleteProfileCommand,
    ) -> ApplicationResult<AthleteProfile> {
        let profile = AthleteProfile::new(
            actor,
            cmd.goals.as_deref(),
            cmd.training_age_months,
            cmd.limitations.as_deref(),
            cmd.date_of_birth,
            cmd.height_cm,
            self.clock.now().date_naive(),
        )?;

        self.profiles.upsert_athlete(&profile).await?;
        Ok(profile)
    }

    pub async fn update_trainer(
        &self,
        actor: UserId,
        cmd: UpdateTrainerProfileCommand,
    ) -> ApplicationResult<TrainerProfile> {
        let profile = TrainerProfile::new(
            actor,
            cmd.headline.as_deref(),
            cmd.bio.as_deref(),
            &cmd.certifications,
            &cmd.specialties,
        )?;

        self.profiles.upsert_trainer(&profile).await?;
        Ok(profile)
    }

    /// Rename the account. The name shows up in rosters, coaching lists and the
    /// audit trail's actor column — which is why it is bounded like any other
    /// name rather than free text.
    pub async fn rename(&self, actor: UserId, display_name: &str) -> ApplicationResult<String> {
        let name = validated_name("display_name", display_name, 50)?;
        self.users.rename(actor, &name).await?;
        Ok(name)
    }

    /// The caller's measurements, newest first.
    pub async fn my_measurements(&self, actor: UserId) -> ApplicationResult<Vec<BodyMeasurement>> {
        self.profiles.measurements(actor).await
    }

    /// Record (or correct) one day's numbers. Upsert by date: a second entry
    /// for the same morning is a correction, not a new fact.
    pub async fn save_measurement(
        &self,
        actor: UserId,
        measured_on: NaiveDate,
        entry: MeasurementEntry,
    ) -> ApplicationResult<BodyMeasurement> {
        let measurement =
            BodyMeasurement::new(actor, measured_on, entry, self.clock.now().date_naive())?;
        self.profiles.upsert_measurement(&measurement).await?;
        Ok(measurement)
    }

    /// Delete one day's entry — your own body data carries an eraser.
    pub async fn delete_measurement(
        &self,
        actor: UserId,
        measured_on: NaiveDate,
    ) -> ApplicationResult<()> {
        if !self.profiles.delete_measurement(actor, measured_on).await? {
            return Err(ApplicationError::NotFound {
                entity: "measurement",
            });
        }
        Ok(())
    }

    /// An athlete's measurements, read by their coach or a gym manager — the
    /// same gate as the athlete profile, because it is the same conversation:
    /// weight trend and waist tape are things you share with your coach, not
    /// with the whole gym floor.
    pub async fn measurements_of(
        &self,
        tenant: &TenantContext,
        athlete: UserId,
    ) -> ApplicationResult<Vec<BodyMeasurement>> {
        let permitted = athlete == tenant.actor_id
            || tenant.capabilities.can_manage_catalogue()
            || self
                .relationships
                .active_for_user(tenant, tenant.actor_id)
                .await?
                .iter()
                .any(|r| r.grants_access_to(tenant.actor_id, athlete));

        if !permitted {
            return Err(ApplicationError::NotFound {
                entity: "measurements",
            });
        }

        self.profiles.measurements(athlete).await
    }

    /// An athlete's profile, read by someone standing over them in THIS gym.
    ///
    /// Limitations and goals are exactly what a coach needs before writing week
    /// one — and exactly what a fellow member has no business reading. Gate:
    /// self, gym managers, or an active coach of this athlete. Everyone else
    /// gets "not found", never confirmation the person trains here.
    pub async fn athlete_profile_of(
        &self,
        tenant: &TenantContext,
        athlete: UserId,
    ) -> ApplicationResult<Option<AthleteProfile>> {
        let permitted = athlete == tenant.actor_id
            || tenant.capabilities.can_manage_catalogue()
            || self
                .relationships
                .active_for_user(tenant, tenant.actor_id)
                .await?
                .iter()
                .any(|r| r.grants_access_to(tenant.actor_id, athlete));

        if !permitted {
            return Err(ApplicationError::NotFound { entity: "profile" });
        }

        self.profiles.athlete_profile(athlete).await
    }
}

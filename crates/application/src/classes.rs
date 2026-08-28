//! Group classes: publishing a timetable, and members booking into it.
//!
//! **Booking is gated on `Feature::GymAccess`, not `ClassCredits`.** The
//! entitlement enum has carried `ClassCredits` as scaffolding since before
//! there were classes, for a per-class-pack balance that still does not exist.
//! Gating on it now would mean every member of every gym — including the gyms
//! that grant `gym_access` and nothing else — is refused a place, which is not
//! what any of the seeded plans intend. `GymAccess` is the honest rule for
//! "may this person train here today", and it is the same rung the door uses.
//! When credit balances arrive, this is the one call site that changes.
//!
//! **Wall-clock, resolved here.** A class starts at 18:00 *in the gym's zone*.
//! Whether that has passed is a decision the server makes when it accepts a
//! booking, so this is where the IANA name on the gym becomes a real instant.
//! Comparing in UTC instead would be wrong for every gym not on UTC, half the
//! year — the exact bug the schema stores a zone name to avoid.

use std::sync::Arc;

use chrono::{NaiveDate, NaiveTime, TimeZone};
use chrono_tz::Tz;
use gym_domain::{
    ClassBookingId, GymClassId, TenantContext, UserId,
    entitlement::Feature,
    gym_class::{ClassBooking, ClassError, GymClass},
};

use crate::{
    ApplicationError, ApplicationResult,
    entitlements::EntitlementService,
    ports::{ClassOnDate, ClassRepository, Clock},
};

/// A timetable request wider than this is a client bug, not a use case. Same
/// spirit as `CalendarService`'s window cap.
const MAX_WINDOW_DAYS: i64 = 62;

#[derive(Clone)]
pub struct ClassService {
    pub classes: Arc<dyn ClassRepository>,
    pub entitlements: EntitlementService,
    pub clock: Arc<dyn Clock>,
}

impl ClassService {
    /// Publishing the timetable is gym management, like plans and opening
    /// hours — not coaching. A trainer teaches a class; they do not decide
    /// that the gym runs one on Mondays.
    fn ensure_manager(&self, tenant: &TenantContext) -> ApplicationResult<()> {
        if tenant.capabilities.can_manage_gym() {
            Ok(())
        } else {
            Err(ApplicationError::Forbidden)
        }
    }

    /// The gym's "now", in its own wall-clock terms.
    ///
    /// An unparseable zone falls back to UTC rather than failing the request:
    /// a typo in a gym's settings should not stop its members booking, and the
    /// column is CHECK-constrained for length, not for being a real zone.
    async fn local_now(&self, tenant: &TenantContext) -> ApplicationResult<(NaiveDate, NaiveTime)> {
        let zone = self.classes.timezone(tenant).await?;
        let tz: Tz = zone.parse().unwrap_or(chrono_tz::UTC);
        let local = tz.from_utc_datetime(&self.clock.now().naive_utc());
        Ok((local.date_naive(), local.time()))
    }

    // ------------------------------------------------------------ the timetable

    /// What is on, and whether the caller is in it.
    ///
    /// Open to everybody in the gym: a member needs it to book, a trainer to
    /// see their own, a manager to run the place. Occupancy is not a secret —
    /// "20/20 FULL" is the single most useful thing on the row.
    pub async fn timetable(
        &self,
        tenant: &TenantContext,
        from: NaiveDate,
        to: NaiveDate,
    ) -> ApplicationResult<Vec<ClassOnDate>> {
        if to < from {
            return Err(ApplicationError::Domain(gym_domain::DomainError::Invalid(
                "the end of the window must not precede its start".to_owned(),
            )));
        }
        if (to - from).num_days() > MAX_WINDOW_DAYS {
            return Err(ApplicationError::Domain(gym_domain::DomainError::Invalid(
                format!("ask for at most {MAX_WINDOW_DAYS} days at a time"),
            )));
        }
        self.classes
            .timetable(tenant, from, to, tenant.actor_id)
            .await
    }

    /// Add a class to the timetable. Managers only.
    #[allow(clippy::too_many_arguments)]
    pub async fn create(
        &self,
        tenant: &TenantContext,
        name: &str,
        instructor_id: UserId,
        weekday: u8,
        starts_at: NaiveTime,
        duration_minutes: u16,
        capacity: u32,
        description: Option<&str>,
    ) -> ApplicationResult<GymClass> {
        self.ensure_manager(tenant)?;

        let class = GymClass::new(
            tenant.gym_id,
            name,
            instructor_id,
            weekday,
            starts_at,
            duration_minutes,
            capacity,
            description,
        )?;

        self.classes.insert_class(tenant, &class).await?;
        Ok(class)
    }

    /// Drop a class from the timetable. Managers only.
    ///
    /// Archived, never deleted: bookings reference it, and "who was in
    /// Tuesday's HIIT" has to stay answerable afterwards.
    pub async fn archive(
        &self,
        tenant: &TenantContext,
        id: GymClassId,
    ) -> ApplicationResult<GymClass> {
        self.ensure_manager(tenant)?;

        let mut class = self
            .classes
            .find_class(tenant, id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "class" })?;

        class.archive(self.clock.now());
        self.classes.save_archived_class(tenant, &class).await?;
        Ok(class)
    }

    // --------------------------------------------------------------- booking

    /// Take a place in one sitting.
    ///
    /// The id comes from the caller (ADR-0008) so a retry replays rather than
    /// double-books. Capacity is read, then the domain decides; the unique
    /// index in `class_bookings` is what makes the rule hold when two people
    /// tap Book in the same instant, and the insert's conflict surfaces as
    /// `StateConflict` (409 with a readable reason) rather than a 500.
    pub async fn book(
        &self,
        tenant: &TenantContext,
        id: ClassBookingId,
        class_id: GymClassId,
        on_date: NaiveDate,
    ) -> ApplicationResult<ClassBooking> {
        // Booking is for yourself. There is no "book somebody else" case worth
        // having — a manager adding a member to a class is a different feature
        // with different consent, and quietly allowing it here would be it.
        self.entitlements
            .require(tenant, tenant.actor_id, Feature::GymAccess)
            .await?;

        let class = self
            .classes
            .find_class(tenant, class_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "class" })?;

        let held = self
            .classes
            .live_booking_count(tenant, class_id, on_date)
            .await?;
        let (local_date, local_time) = self.local_now(tenant).await?;

        let booking = ClassBooking::book(
            id,
            &class,
            tenant.actor_id,
            on_date,
            held,
            local_date,
            local_time,
            self.clock.now(),
        )?;

        self.classes.insert_booking(tenant, &booking).await?;
        Ok(booking)
    }

    /// Give a place back.
    ///
    /// Your own only. A member cancelling somebody else's place is not a
    /// permission gap to leave open, and a manager needing to do it is a
    /// different feature.
    pub async fn cancel_booking(
        &self,
        tenant: &TenantContext,
        id: ClassBookingId,
    ) -> ApplicationResult<ClassBooking> {
        let mut booking = self
            .classes
            .find_booking(tenant, id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "booking" })?;

        // Not-found rather than forbidden: whether somebody else's booking id
        // exists is not a thing to confirm.
        if booking.member_id != tenant.actor_id {
            return Err(ApplicationError::NotFound { entity: "booking" });
        }

        let class = self
            .classes
            .find_class(tenant, booking.class_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "class" })?;

        let (local_date, local_time) = self.local_now(tenant).await?;
        booking.cancel(&class, local_date, local_time, self.clock.now())?;

        self.classes.save_cancelled_booking(tenant, &booking).await?;
        Ok(booking)
    }

    /// Who is in one sitting.
    ///
    /// The class's own instructor, or a manager. Not every trainer: a roster is
    /// a list of members' names and attendance, and a trainer with no
    /// involvement in that class has no reason to read it.
    pub async fn roster(
        &self,
        tenant: &TenantContext,
        class_id: GymClassId,
        on_date: NaiveDate,
    ) -> ApplicationResult<Vec<(UserId, String)>> {
        let class = self
            .classes
            .find_class(tenant, class_id)
            .await?
            .ok_or(ApplicationError::NotFound { entity: "class" })?;

        if !tenant.capabilities.can_manage_gym() && class.instructor_id != tenant.actor_id {
            return Err(ApplicationError::Forbidden);
        }

        self.classes.roster(tenant, class_id, on_date).await
    }
}

impl From<ClassError> for ApplicationError {
    fn from(err: ClassError) -> Self {
        match err {
            // Well-formed request, conflicting state: the caller should look
            // again rather than rewrite. "Full" and "already started" are the
            // two a client retries into, so they must not read as 400s.
            ClassError::Full { .. } | ClassError::AlreadyStarted | ClassError::AlreadyCancelled => {
                Self::StateConflict(err.to_string())
            }
            // These are the caller asking for something incoherent.
            ClassError::WrongWeekday { .. } | ClassError::Archived { .. } => {
                Self::Domain(gym_domain::DomainError::Invalid(err.to_string()))
            }
        }
    }
}

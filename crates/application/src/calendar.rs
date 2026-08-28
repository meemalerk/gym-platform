//! The operating calendar use-cases (ADR-0015).
//!
//! Reading is open to everyone in the gym — "are you open on Friday?" is not a
//! privileged question, and a member who cannot see the hours cannot plan a
//! session. Writing is `can_manage_gym`: opening hours are a business fact
//! about the premises, not a coaching decision, so a head coach does not set
//! them. Availability is the exception, and the interesting one: a trainer
//! sets their own.

use std::sync::Arc;

use chrono::{Days, NaiveDate};
use gym_domain::{
    TenantContext, UserId,
    calendar::{CalendarOverride, OpeningDay, TimeSpan, WeeklyHours, bookable_spans, resolve_day},
};

use crate::{
    ApplicationError, ApplicationResult,
    ports::{CalendarRepository, UserRepository},
};

/// The longest window a caller may ask for in one go.
///
/// 400 days covers "the year ahead" — which is what a gym planning holidays
/// actually wants — while stopping a request for the next century from
/// generating four hundred thousand rows to serialise.
const MAX_WINDOW_DAYS: u64 = 400;

#[derive(Clone)]
pub struct CalendarService {
    pub calendar: Arc<dyn CalendarRepository>,
    pub users: Arc<dyn UserRepository>,
}

/// A resolved stretch of days, plus the zone the times are in.
pub struct ResolvedCalendar {
    /// IANA name. The times in `days` are wall-clock in this zone — a client
    /// converting them to instants needs it, and getting it from somewhere
    /// else would be getting it from a guess.
    pub timezone: String,
    pub days: Vec<OpeningDay>,
}

impl CalendarService {
    /// The weekly pattern.
    pub async fn opening_hours(
        &self,
        tenant: &TenantContext,
    ) -> ApplicationResult<Vec<WeeklyHours>> {
        self.calendar.opening_hours(tenant).await
    }

    /// Resolve every day in a window.
    ///
    /// Reads the pattern and the overrides once each, then applies
    /// `resolve_day` per date. The rule is not re-implemented here — this
    /// function's entire job is to call the one that has it.
    pub async fn resolve(
        &self,
        tenant: &TenantContext,
        from: NaiveDate,
        to: NaiveDate,
    ) -> ApplicationResult<ResolvedCalendar> {
        if to < from {
            return Err(ApplicationError::Domain(gym_domain::DomainError::Invalid(
                "the end of the window must not precede its start".to_owned(),
            )));
        }

        let span_days = u64::try_from((to - from).num_days()).unwrap_or(0);
        if span_days > MAX_WINDOW_DAYS {
            return Err(ApplicationError::Domain(gym_domain::DomainError::Invalid(
                format!("ask for at most {MAX_WINDOW_DAYS} days at a time"),
            )));
        }

        let pattern = self.calendar.opening_hours(tenant).await?;
        let overrides = self.calendar.overrides_between(tenant, from, to).await?;
        let timezone = self.calendar.timezone(tenant).await?;

        let mut days = Vec::with_capacity(usize::try_from(span_days + 1).unwrap_or(1));
        let mut date = from;
        while date <= to {
            days.push(resolve_day(date, &pattern, &overrides));
            let Some(next) = date.checked_add_days(Days::new(1)) else {
                break;
            };
            date = next;
        }

        Ok(ResolvedCalendar { timezone, days })
    }

    /// Replace the weekly pattern. Owners and admins.
    pub async fn set_opening_hours(
        &self,
        tenant: &TenantContext,
        spans: Vec<(u8, TimeSpan)>,
    ) -> ApplicationResult<Vec<WeeklyHours>> {
        Self::ensure_manager(tenant)?;

        let hours: Vec<WeeklyHours> = spans
            .into_iter()
            .map(|(weekday, span)| {
                if weekday > 6 {
                    return Err(ApplicationError::Domain(gym_domain::DomainError::Invalid(
                        "weekday must be 0 (Sunday) to 6 (Saturday)".to_owned(),
                    )));
                }
                Ok(WeeklyHours {
                    id: gym_domain::CalendarEntryId::new(),
                    gym_id: tenant.gym_id,
                    weekday,
                    span,
                })
            })
            .collect::<ApplicationResult<_>>()?;

        self.calendar.replace_opening_hours(tenant, &hours).await?;
        Ok(hours)
    }

    /// Close a date, or give it special hours.
    pub async fn set_override(
        &self,
        tenant: &TenantContext,
        entry: CalendarOverride,
    ) -> ApplicationResult<()> {
        Self::ensure_manager(tenant)?;
        self.calendar.upsert_override(tenant, &entry).await
    }

    /// Drop an override so the weekly pattern applies again.
    pub async fn clear_override(
        &self,
        tenant: &TenantContext,
        on_date: NaiveDate,
    ) -> ApplicationResult<()> {
        Self::ensure_manager(tenant)?;

        if self.calendar.remove_override(tenant, on_date).await? {
            Ok(())
        } else {
            Err(ApplicationError::NotFound {
                entity: "calendar override",
            })
        }
    }

    /// One trainer's availability.
    pub async fn availability(
        &self,
        tenant: &TenantContext,
        trainer: UserId,
    ) -> ApplicationResult<Vec<WeeklyHours>> {
        self.calendar.trainer_availability(tenant, trainer).await
    }

    /// Set your own availability, or a manager sets somebody's.
    ///
    /// The one write on the calendar that is not manager-only. A trainer
    /// deciding when they work is not a business decision about the premises,
    /// and making them ask an owner to change their Tuesday would guarantee
    /// the data goes stale.
    pub async fn set_availability(
        &self,
        tenant: &TenantContext,
        trainer: UserId,
        spans: Vec<(u8, TimeSpan)>,
    ) -> ApplicationResult<Vec<WeeklyHours>> {
        let own = trainer == tenant.actor_id;
        if !own && !tenant.capabilities.can_manage_catalogue() {
            return Err(ApplicationError::Forbidden);
        }

        // Whoever it is for must actually coach here — otherwise availability
        // accumulates for people who cannot be booked, and the directory grows
        // rows nobody can use.
        let capabilities = self.users.capabilities_in(trainer, tenant.gym_id).await?;
        if !capabilities.can_coach() {
            return Err(ApplicationError::NotFound { entity: "coach" });
        }

        let hours: Vec<WeeklyHours> = spans
            .into_iter()
            .map(|(weekday, span)| {
                if weekday > 6 {
                    return Err(ApplicationError::Domain(gym_domain::DomainError::Invalid(
                        "weekday must be 0 (Sunday) to 6 (Saturday)".to_owned(),
                    )));
                }
                Ok(WeeklyHours {
                    id: gym_domain::CalendarEntryId::new(),
                    gym_id: tenant.gym_id,
                    weekday,
                    span,
                })
            })
            .collect::<ApplicationResult<_>>()?;

        self.calendar
            .replace_trainer_availability(tenant, trainer, &hours)
            .await?;
        Ok(hours)
    }

    /// When a trainer can actually be booked over a window.
    ///
    /// The intersection of their availability with the gym being open — the
    /// reason both are stored in the same shape.
    pub async fn bookable(
        &self,
        tenant: &TenantContext,
        trainer: UserId,
        from: NaiveDate,
        to: NaiveDate,
    ) -> ApplicationResult<Vec<(NaiveDate, Vec<TimeSpan>)>> {
        let resolved = self.resolve(tenant, from, to).await?;
        let availability = self.calendar.trainer_availability(tenant, trainer).await?;

        Ok(resolved
            .days
            .into_iter()
            .map(|day| {
                let spans = bookable_spans(day.date, &day, &availability);
                (day.date, spans)
            })
            .collect())
    }

    fn ensure_manager(tenant: &TenantContext) -> ApplicationResult<()> {
        // `can_manage_gym`, deliberately narrower than `can_manage_catalogue`:
        // when the building is open is a fact about the business, not a
        // coaching decision, and a head coach should not be able to close it.
        if tenant.capabilities.can_manage_gym() {
            Ok(())
        } else {
            Err(ApplicationError::Forbidden)
        }
    }
}

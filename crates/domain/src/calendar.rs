//! The gym's operating calendar (ADR-0015).
//!
//! **One resolution rule, in one function.** For a given date: if an override
//! exists it wins entirely — either closed, or exactly those hours. Otherwise
//! the weekly pattern applies. No merging, no partial override, no precedence
//! ladder. Everything that wants to know whether the gym is open calls
//! `resolve_day`, and there is deliberately nowhere else the question can be
//! answered.
//!
//! Times are wall-clock (`NaiveTime`), never instants. "Opens at 06:00" is a
//! fact about a place, and it survives DST precisely because it was never an
//! instant to begin with. The timezone lives on the gym and is applied at the
//! edge, when a wall-clock time has to become a moment.

use chrono::{Datelike, NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};

use crate::{DomainError, GymId, ids::CalendarEntryId};

/// One span of open time. The same shape for a gym's hours and a trainer's
/// availability, deliberately — see the migration's comment on why.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct TimeSpan {
    #[schema(value_type = String, example = "06:00:00")]
    pub opens_at: NaiveTime,
    #[schema(value_type = String, example = "22:00:00")]
    pub closes_at: NaiveTime,
}

impl TimeSpan {
    pub fn new(opens_at: NaiveTime, closes_at: NaiveTime) -> Result<Self, DomainError> {
        if closes_at <= opens_at {
            return Err(DomainError::Invalid(
                "closing time must be after opening time".to_owned(),
            ));
        }
        Ok(Self {
            opens_at,
            closes_at,
        })
    }

    #[must_use]
    pub fn contains(&self, at: NaiveTime) -> bool {
        // Half-open: a gym closing at 22:00 is shut AT 22:00. Inclusive on both
        // ends would make two adjacent spans overlap by an instant, and
        // "bookable at closing time" is not a thing anyone wants.
        at >= self.opens_at && at < self.closes_at
    }

    /// The part of this span that is also inside `other`, if any.
    ///
    /// The whole reason gym hours and trainer availability share a shape: a
    /// trainer's bookable time is this operation, and it is two comparisons.
    #[must_use]
    pub fn intersect(&self, other: &Self) -> Option<Self> {
        let opens_at = self.opens_at.max(other.opens_at);
        let closes_at = self.closes_at.min(other.closes_at);
        (closes_at > opens_at).then_some(Self {
            opens_at,
            closes_at,
        })
    }
}

/// A recurring weekly entry — the gym's pattern, or one trainer's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WeeklyHours {
    pub id: CalendarEntryId,
    pub gym_id: GymId,
    /// 0 = Sunday, matching Postgres `EXTRACT(DOW)` and JS `getDay()`.
    pub weekday: u8,
    #[serde(flatten)]
    pub span: TimeSpan,
}

/// A date that differs from the pattern.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct CalendarOverride {
    pub id: CalendarEntryId,
    pub gym_id: GymId,
    pub on_date: NaiveDate,
    /// Explicit, never inferred from absent hours: "closed for Eid" and "hours
    /// not configured" are different states, and confusing them either turns
    /// people away or lets them into a locked building.
    pub is_closed: bool,
    /// `None` exactly when `is_closed`.
    pub span: Option<TimeSpan>,
    pub reason: Option<String>,
}

impl CalendarOverride {
    pub fn closed(
        gym_id: GymId,
        on_date: NaiveDate,
        reason: Option<&str>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id: CalendarEntryId::new(),
            gym_id,
            on_date,
            is_closed: true,
            span: None,
            reason: clean_reason(reason)?,
        })
    }

    pub fn special_hours(
        gym_id: GymId,
        on_date: NaiveDate,
        span: TimeSpan,
        reason: Option<&str>,
    ) -> Result<Self, DomainError> {
        Ok(Self {
            id: CalendarEntryId::new(),
            gym_id,
            on_date,
            is_closed: false,
            span: Some(span),
            reason: clean_reason(reason)?,
        })
    }
}

fn clean_reason(reason: Option<&str>) -> Result<Option<String>, DomainError> {
    match reason.map(str::trim) {
        None | Some("") => Ok(None),
        Some(text) => Ok(Some(crate::validated_name("reason", text, 120)?)),
    }
}

/// What one day looks like, once the rule has been applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct OpeningDay {
    pub date: NaiveDate,
    /// Empty when closed. Sorted, so a UI can render them in order without
    /// re-sorting and without wondering whether it needs to.
    pub spans: Vec<TimeSpan>,
    /// True only when an override says so — not merely because `spans` is
    /// empty, which also happens when nobody has configured that weekday.
    pub closed_by_override: bool,
    pub reason: Option<String>,
}

impl OpeningDay {
    #[must_use]
    pub fn is_open_at(&self, at: NaiveTime) -> bool {
        self.spans.iter().any(|s| s.contains(at))
    }

    /// Open at some point today. Distinct from `is_open_at` — a gym with a
    /// split day is "open today" at 12:00 while being shut at that moment.
    #[must_use]
    pub fn opens_at_all(&self) -> bool {
        !self.spans.is_empty()
    }
}

/// **The** resolution rule. Everything asks this; nothing re-implements it.
///
/// An override wins entirely. That is the whole rule, and its value is that it
/// is one sentence: any merging behaviour ("special hours extend the pattern")
/// would need a precedence ladder, and a ladder is a thing people get wrong at
/// the edges and argue about in review.
#[must_use]
pub fn resolve_day(
    date: NaiveDate,
    pattern: &[WeeklyHours],
    overrides: &[CalendarOverride],
) -> OpeningDay {
    if let Some(exception) = overrides.iter().find(|o| o.on_date == date) {
        return OpeningDay {
            date,
            spans: exception.span.into_iter().collect(),
            closed_by_override: exception.is_closed,
            reason: exception.reason.clone(),
        };
    }

    // `num_days_from_sunday` matches the 0 = Sunday convention the column
    // documents. Converting between conventions in two places is how a Monday
    // becomes a Sunday.
    let weekday = u8::try_from(date.weekday().num_days_from_sunday()).unwrap_or(0);

    let mut spans: Vec<TimeSpan> = pattern
        .iter()
        .filter(|h| h.weekday == weekday)
        .map(|h| h.span)
        .collect();
    spans.sort_by_key(|s| s.opens_at);

    OpeningDay {
        date,
        spans,
        closed_by_override: false,
        reason: None,
    }
}

/// When a trainer can actually be booked on `date`.
///
/// The intersection of their availability and the gym being open. A trainer
/// free at 05:00 in a gym that opens at 06:00 is not bookable at 05:00, and
/// this is the only place that has to know it.
#[must_use]
pub fn bookable_spans(
    date: NaiveDate,
    gym: &OpeningDay,
    trainer_pattern: &[WeeklyHours],
) -> Vec<TimeSpan> {
    // A gym override closing the building closes it for everybody. A trainer's
    // own availability cannot open a shut gym.
    if !gym.opens_at_all() {
        return Vec::new();
    }

    let weekday = u8::try_from(date.weekday().num_days_from_sunday()).unwrap_or(0);

    let mut spans: Vec<TimeSpan> = trainer_pattern
        .iter()
        .filter(|h| h.weekday == weekday)
        .flat_map(|h| gym.spans.iter().filter_map(move |g| h.span.intersect(g)))
        .collect();

    spans.sort_by_key(|s| s.opens_at);
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn span(from: (u32, u32), to: (u32, u32)) -> TimeSpan {
        TimeSpan::new(t(from.0, from.1), t(to.0, to.1)).unwrap()
    }

    fn weekly(weekday: u8, from: (u32, u32), to: (u32, u32)) -> WeeklyHours {
        WeeklyHours {
            id: CalendarEntryId::new(),
            gym_id: GymId::new(),
            weekday,
            span: span(from, to),
        }
    }

    /// 2026-08-24 is a Monday; 2026-08-23 a Sunday.
    fn monday() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 24).unwrap()
    }
    fn sunday() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 23).unwrap()
    }

    #[test]
    fn a_span_is_half_open() {
        let s = span((6, 0), (22, 0));
        assert!(s.contains(t(6, 0)), "open at opening time");
        assert!(s.contains(t(21, 59)));
        assert!(!s.contains(t(22, 0)), "shut AT closing time");
        assert!(!s.contains(t(5, 59)));
    }

    #[test]
    fn closing_before_opening_is_refused() {
        assert!(TimeSpan::new(t(22, 0), t(6, 0)).is_err());
        assert!(TimeSpan::new(t(6, 0), t(6, 0)).is_err(), "zero length");
    }

    #[test]
    fn the_weekly_pattern_applies_when_nothing_overrides_it() {
        let pattern = vec![weekly(1, (6, 0), (22, 0))];
        let day = resolve_day(monday(), &pattern, &[]);
        assert_eq!(day.spans, vec![span((6, 0), (22, 0))]);
        assert!(day.opens_at_all());
    }

    #[test]
    fn a_day_with_no_pattern_is_shut_but_not_overridden() {
        // The distinction the `closed_by_override` flag exists for: nobody
        // configured Sunday, which is not the same as somebody closing it.
        let day = resolve_day(sunday(), &[weekly(1, (6, 0), (22, 0))], &[]);
        assert!(!day.opens_at_all());
        assert!(!day.closed_by_override);
        assert!(day.reason.is_none());
    }

    #[test]
    fn split_hours_come_back_in_order() {
        // Listed evening-first on purpose — the resolver sorts, so no caller
        // has to wonder whether it needs to.
        let pattern = vec![weekly(1, (16, 0), (22, 0)), weekly(1, (6, 0), (10, 0))];
        let day = resolve_day(monday(), &pattern, &[]);
        assert_eq!(
            day.spans,
            vec![span((6, 0), (10, 0)), span((16, 0), (22, 0))]
        );
        assert!(day.is_open_at(t(7, 0)));
        assert!(!day.is_open_at(t(12, 0)), "shut between the two shifts");
        assert!(day.is_open_at(t(18, 0)));
    }

    #[test]
    fn an_override_wins_entirely() {
        let gym = GymId::new();
        let pattern = vec![weekly(1, (6, 0), (22, 0))];
        let overrides = vec![
            CalendarOverride::special_hours(gym, monday(), span((9, 0), (13, 0)), Some("Eid"))
                .unwrap(),
        ];

        let day = resolve_day(monday(), &pattern, &overrides);

        // NOT merged with the pattern, and not extending it. Entirely replaced.
        assert_eq!(day.spans, vec![span((9, 0), (13, 0))]);
        assert!(
            !day.is_open_at(t(7, 0)),
            "the pattern does not leak through"
        );
        assert_eq!(day.reason.as_deref(), Some("Eid"));
    }

    #[test]
    fn a_closure_override_shuts_the_day_and_says_why() {
        let gym = GymId::new();
        let pattern = vec![weekly(1, (6, 0), (22, 0))];
        let overrides =
            vec![CalendarOverride::closed(gym, monday(), Some("Public holiday")).unwrap()];

        let day = resolve_day(monday(), &pattern, &overrides);
        assert!(!day.opens_at_all());
        assert!(day.closed_by_override);
        assert_eq!(day.reason.as_deref(), Some("Public holiday"));
    }

    #[test]
    fn an_override_for_another_date_is_ignored() {
        let gym = GymId::new();
        let pattern = vec![weekly(1, (6, 0), (22, 0))];
        let overrides = vec![CalendarOverride::closed(gym, sunday(), None).unwrap()];

        let day = resolve_day(monday(), &pattern, &overrides);
        assert_eq!(day.spans, vec![span((6, 0), (22, 0))]);
    }

    #[test]
    fn intersection_is_the_overlap_or_nothing() {
        assert_eq!(
            span((6, 0), (12, 0)).intersect(&span((9, 0), (18, 0))),
            Some(span((9, 0), (12, 0)))
        );
        assert_eq!(span((6, 0), (9, 0)).intersect(&span((9, 0), (18, 0))), None,);
        assert_eq!(
            span((6, 0), (22, 0)).intersect(&span((9, 0), (10, 0))),
            Some(span((9, 0), (10, 0))),
            "fully contained"
        );
    }

    #[test]
    fn a_trainer_is_bookable_only_while_the_gym_is_open() {
        let pattern = vec![weekly(1, (6, 0), (22, 0))];
        let gym = resolve_day(monday(), &pattern, &[]);
        // Available from 05:00, an hour before the doors open.
        let trainer = vec![weekly(1, (5, 0), (12, 0))];

        assert_eq!(
            bookable_spans(monday(), &gym, &trainer),
            vec![span((6, 0), (12, 0))],
            "the hour before opening is not bookable"
        );
    }

    #[test]
    fn a_closed_gym_makes_nobody_bookable() {
        // A trainer's own availability cannot open a shut building.
        let gym_id = GymId::new();
        let pattern = vec![weekly(1, (6, 0), (22, 0))];
        let overrides = vec![CalendarOverride::closed(gym_id, monday(), Some("Flooded")).unwrap()];
        let gym = resolve_day(monday(), &pattern, &overrides);

        assert!(bookable_spans(monday(), &gym, &[weekly(1, (9, 0), (17, 0))]).is_empty());
    }

    #[test]
    fn availability_is_clipped_to_each_shift_of_a_split_day() {
        let pattern = vec![weekly(1, (6, 0), (10, 0)), weekly(1, (16, 0), (22, 0))];
        let gym = resolve_day(monday(), &pattern, &[]);
        // Available all day; bookable only during the two shifts.
        let trainer = vec![weekly(1, (0, 1), (23, 59))];

        assert_eq!(
            bookable_spans(monday(), &gym, &trainer),
            vec![span((6, 0), (10, 0)), span((16, 0), (22, 0))]
        );
    }

    #[test]
    fn a_trainer_free_on_another_day_is_not_bookable_today() {
        let pattern = vec![weekly(1, (6, 0), (22, 0))];
        let gym = resolve_day(monday(), &pattern, &[]);
        assert!(bookable_spans(monday(), &gym, &[weekly(2, (9, 0), (17, 0))]).is_empty());
    }

    #[test]
    fn a_blank_reason_normalises_to_none() {
        let o = CalendarOverride::closed(GymId::new(), monday(), Some("   ")).unwrap();
        assert!(o.reason.is_none());
    }
}

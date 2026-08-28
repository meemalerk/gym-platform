//! Group classes, and holding a place in one.
//!
//! **A class is a weekly slot; a booking is a place in one date's sitting of
//! it.** The timetable says "Zumba, Mondays at 18:00, cap 20" once, and every
//! Monday is derived from it rather than stored. That split is the whole design
//! (see the migration for why generating occurrences ahead is worse), and it is
//! why `ClassOccurrence` exists here: it is the pairing of a slot with a date,
//! which is the only thing a member can actually book.
//!
//! Times are wall-clock, like the operating calendar. "18:00 Zumba" is a fact
//! about a place, and it survives DST because it was never an instant. Turning
//! one into a moment needs the gym's timezone and happens at the edge.
//!
//! The capacity rule takes the live count as an argument rather than reaching
//! for it. Counting is the database's job and it is racy by nature — two
//! members tapping Book at once — so the domain states the rule and the unique
//! index in `class_bookings` is what makes it true under concurrency.

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{ClassBookingId, DomainError, GymClassId, GymId, UserId};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ClassError {
    /// The date does not fall on the class's weekday. A Tuesday booking for a
    /// Monday class is a bug in the caller, not a preference to honour.
    #[error("{name} runs on {expected}, not {given}")]
    WrongWeekday {
        name: String,
        expected: &'static str,
        given: &'static str,
    },

    #[error("that sitting has already started")]
    AlreadyStarted,

    #[error("{name} is full ({capacity} places)")]
    Full { name: String, capacity: u32 },

    #[error("that booking is already cancelled")]
    AlreadyCancelled,

    /// A class the gym has dropped from its timetable. Still readable — old
    /// bookings reference it — but nobody new gets in.
    #[error("{name} is no longer on the timetable")]
    Archived { name: String },
}

/// 0 = Sunday, matching `gym_opening_hours`, Postgres `EXTRACT(DOW)` and
/// JavaScript `getDay()`. Converting between conventions in more than one place
/// is how a Monday becomes a Sunday, so there is exactly one spelling of it.
#[must_use]
pub fn weekday_of(date: NaiveDate) -> u8 {
    u8::try_from(date.weekday().num_days_from_sunday()).unwrap_or(0)
}

#[must_use]
pub fn weekday_name(weekday: u8) -> &'static str {
    match weekday % 7 {
        0 => "Sunday",
        1 => "Monday",
        2 => "Tuesday",
        3 => "Wednesday",
        4 => "Thursday",
        5 => "Friday",
        _ => "Saturday",
    }
}

/// A recurring weekly class slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct GymClass {
    pub id: GymClassId,
    pub gym_id: GymId,
    pub name: String,
    pub instructor_id: UserId,
    /// 0 = Sunday.
    pub weekday: u8,
    #[schema(value_type = String, example = "18:00:00")]
    pub starts_at: NaiveTime,
    pub duration_minutes: u16,
    pub capacity: u32,
    #[schema(nullable)]
    pub description: Option<String>,
    pub archived_at: Option<DateTime<Utc>>,
}

impl GymClass {
    /// Create a slot, validating everything the database also checks.
    ///
    /// Both layers on purpose: the CHECK constraints are the guarantee, and
    /// these are the readable error a person gets instead of a 500.
    pub fn new(
        gym_id: GymId,
        name: &str,
        instructor_id: UserId,
        weekday: u8,
        starts_at: NaiveTime,
        duration_minutes: u16,
        capacity: u32,
        description: Option<&str>,
    ) -> Result<Self, DomainError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DomainError::Empty { field: "name" });
        }
        if name.chars().count() > 80 {
            return Err(DomainError::TooLong {
                field: "name",
                max: 80,
                actual: name.chars().count(),
            });
        }
        if weekday > 6 {
            return Err(DomainError::Invalid(
                "weekday must be 0 (Sunday) through 6 (Saturday)".to_owned(),
            ));
        }
        if !(5..=300).contains(&duration_minutes) {
            return Err(DomainError::Invalid(
                "a class runs between 5 and 300 minutes".to_owned(),
            ));
        }
        if !(1..=500).contains(&capacity) {
            return Err(DomainError::Invalid(
                "capacity must be between 1 and 500".to_owned(),
            ));
        }
        let description = match description.map(str::trim).filter(|d| !d.is_empty()) {
            Some(d) if d.chars().count() > 300 => {
                return Err(DomainError::TooLong {
                    field: "description",
                    max: 300,
                    actual: d.chars().count(),
                });
            }
            other => other.map(ToOwned::to_owned),
        };

        Ok(Self {
            id: GymClassId::new(),
            gym_id,
            name: name.to_owned(),
            instructor_id,
            weekday,
            starts_at,
            duration_minutes,
            capacity,
            description,
            archived_at: None,
        })
    }

    #[must_use]
    pub fn is_live(&self) -> bool {
        self.archived_at.is_none()
    }

    /// Drop it from the timetable. Idempotent: archiving twice is not an error
    /// worth a round trip, and the first timestamp is the true one.
    pub fn archive(&mut self, now: DateTime<Utc>) {
        if self.archived_at.is_none() {
            self.archived_at = Some(now);
        }
    }

    /// The next date this class runs, on or after `from`.
    ///
    /// Used to turn a timetable into "this week": the slot itself has no dates,
    /// so every screen that shows one needs this.
    #[must_use]
    pub fn next_date_from(&self, from: NaiveDate) -> NaiveDate {
        let today = i64::from(weekday_of(from));
        let target = i64::from(self.weekday);
        let ahead = (target - today).rem_euclid(7);
        from + chrono::Duration::days(ahead)
    }

    /// Pair this slot with a date to get something bookable.
    pub fn occurrence_on(&self, date: NaiveDate) -> Result<ClassOccurrence, ClassError> {
        if weekday_of(date) != self.weekday {
            return Err(ClassError::WrongWeekday {
                name: self.name.clone(),
                expected: weekday_name(self.weekday),
                given: weekday_name(weekday_of(date)),
            });
        }
        Ok(ClassOccurrence {
            class_id: self.id,
            date,
            starts_at: self.starts_at,
        })
    }
}

/// One sitting of a class: the slot plus the date it runs on. Never stored —
/// derived wherever a date is in hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassOccurrence {
    pub class_id: GymClassId,
    pub date: NaiveDate,
    pub starts_at: NaiveTime,
}

impl ClassOccurrence {
    /// Has it begun? Compared in the gym's wall-clock terms, so the caller
    /// passes the local date and time rather than an instant.
    #[must_use]
    pub fn has_started(&self, local_date: NaiveDate, local_time: NaiveTime) -> bool {
        (self.date, self.starts_at) <= (local_date, local_time)
    }
}

/// One member's place in one occurrence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct ClassBooking {
    pub id: ClassBookingId,
    pub gym_id: GymId,
    pub class_id: GymClassId,
    pub member_id: UserId,
    pub on_date: NaiveDate,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl ClassBooking {
    /// Take a place.
    ///
    /// `live_bookings` is the count already held for this occurrence, passed in
    /// rather than fetched: the rule belongs here, the counting belongs to the
    /// repository, and the unique index is what settles a genuine race.
    ///
    /// The id is the CALLER's (ADR-0008), so a retry after a network wobble
    /// replays the same booking and the insert is a no-op instead of a second
    /// place.
    #[allow(clippy::too_many_arguments)]
    pub fn book(
        id: ClassBookingId,
        class: &GymClass,
        member_id: UserId,
        on_date: NaiveDate,
        live_bookings: u32,
        local_date: NaiveDate,
        local_time: NaiveTime,
        now: DateTime<Utc>,
    ) -> Result<Self, ClassError> {
        if !class.is_live() {
            return Err(ClassError::Archived {
                name: class.name.clone(),
            });
        }

        let occurrence = class.occurrence_on(on_date)?;

        // A sitting you cannot attend is not a place worth holding. Checked
        // before capacity so "that already ran" beats "that is full", which is
        // the more useful of the two answers.
        if occurrence.has_started(local_date, local_time) {
            return Err(ClassError::AlreadyStarted);
        }

        if live_bookings >= class.capacity {
            return Err(ClassError::Full {
                name: class.name.clone(),
                capacity: class.capacity,
            });
        }

        Ok(Self {
            id,
            gym_id: class.gym_id,
            class_id: class.id,
            member_id,
            on_date,
            cancelled_at: None,
            created_at: now,
        })
    }

    #[must_use]
    pub fn is_live(&self) -> bool {
        self.cancelled_at.is_none()
    }

    /// Give the place back.
    ///
    /// Allowed right up to the moment the class starts — a member who cannot
    /// make it should be able to free the place for somebody else, and a gym
    /// that punishes them for saying so gets told nothing at all. After it has
    /// started there is nothing left to release.
    pub fn cancel(
        &mut self,
        class: &GymClass,
        local_date: NaiveDate,
        local_time: NaiveTime,
        now: DateTime<Utc>,
    ) -> Result<(), ClassError> {
        if self.cancelled_at.is_some() {
            return Err(ClassError::AlreadyCancelled);
        }
        let occurrence = class.occurrence_on(self.on_date)?;
        if occurrence.has_started(local_date, local_time) {
            return Err(ClassError::AlreadyStarted);
        }
        self.cancelled_at = Some(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-26T09:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn at(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn date(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    /// 2026-08-31 is a Monday; 2026-09-01 a Tuesday.
    fn monday_class(capacity: u32) -> GymClass {
        GymClass::new(
            GymId::new(),
            "Zumba",
            UserId::new(),
            1,
            at(18, 0),
            45,
            capacity,
            Some("Bring water."),
        )
        .unwrap()
    }

    #[test]
    fn a_class_needs_a_name_a_sane_length_and_a_real_capacity() {
        let gym = GymId::new();
        let who = UserId::new();
        assert!(GymClass::new(gym, "   ", who, 1, at(18, 0), 45, 20, None).is_err());
        assert!(GymClass::new(gym, "Yoga", who, 7, at(18, 0), 45, 20, None).is_err());
        assert!(GymClass::new(gym, "Yoga", who, 1, at(18, 0), 4, 20, None).is_err());
        assert!(GymClass::new(gym, "Yoga", who, 1, at(18, 0), 301, 20, None).is_err());
        assert!(GymClass::new(gym, "Yoga", who, 1, at(18, 0), 45, 0, None).is_err());
        assert!(GymClass::new(gym, "Yoga", who, 1, at(18, 0), 45, 501, None).is_err());
        assert!(GymClass::new(gym, "Yoga", who, 0, at(6, 30), 60, 1, None).is_ok());
    }

    #[test]
    fn weekday_zero_is_sunday_everywhere() {
        // 2026-08-30 is a Sunday. If this ever flips, every timetable row moves
        // by a day — which is precisely the bug the shared convention prevents.
        assert_eq!(weekday_of(date("2026-08-30")), 0);
        assert_eq!(weekday_of(date("2026-08-31")), 1);
        assert_eq!(weekday_name(0), "Sunday");
        assert_eq!(weekday_name(6), "Saturday");
    }

    #[test]
    fn the_next_sitting_is_today_when_today_is_the_day() {
        let class = monday_class(20);
        // From that Monday itself, the next Monday is that same day — a class
        // later today is the one you want to book, not one in a week.
        assert_eq!(class.next_date_from(date("2026-08-31")), date("2026-08-31"));
        // From the Tuesday after, it is six days on.
        assert_eq!(class.next_date_from(date("2026-09-01")), date("2026-09-07"));
    }

    #[test]
    fn a_date_that_is_not_the_class_weekday_is_refused() {
        let class = monday_class(20);
        assert!(class.occurrence_on(date("2026-08-31")).is_ok());
        assert!(matches!(
            class.occurrence_on(date("2026-09-01")),
            Err(ClassError::WrongWeekday { .. })
        ));
    }

    #[test]
    fn booking_fills_up_and_then_refuses() {
        let class = monday_class(2);
        let monday = date("2026-08-31");
        for held in 0..2 {
            assert!(
                ClassBooking::book(
                    ClassBookingId::new(),
                    &class,
                    UserId::new(),
                    monday,
                    held,
                    date("2026-08-26"),
                    at(9, 0),
                    now(),
                )
                .is_ok(),
                "{held} held, capacity 2 — should fit"
            );
        }
        assert!(matches!(
            ClassBooking::book(
                ClassBookingId::new(),
                &class,
                UserId::new(),
                monday,
                2,
                date("2026-08-26"),
                at(9, 0),
                now(),
            ),
            Err(ClassError::Full { capacity: 2, .. })
        ));
    }

    #[test]
    fn a_sitting_that_has_started_cannot_be_booked() {
        let class = monday_class(20);
        let monday = date("2026-08-31");
        // 18:00 on the day, on the dot: started.
        assert!(matches!(
            ClassBooking::book(
                ClassBookingId::new(),
                &class,
                UserId::new(),
                monday,
                0,
                monday,
                at(18, 0),
                now(),
            ),
            Err(ClassError::AlreadyStarted)
        ));
        // A minute before is still open.
        assert!(
            ClassBooking::book(
                ClassBookingId::new(),
                &class,
                UserId::new(),
                monday,
                0,
                monday,
                at(17, 59),
                now(),
            )
            .is_ok()
        );
    }

    /// "Already ran" is more useful than "full", so it is reported first even
    /// when both are true.
    #[test]
    fn a_started_and_full_sitting_says_it_has_started() {
        let class = monday_class(1);
        let monday = date("2026-08-31");
        assert!(matches!(
            ClassBooking::book(
                ClassBookingId::new(),
                &class,
                UserId::new(),
                monday,
                5,
                monday,
                at(19, 0),
                now(),
            ),
            Err(ClassError::AlreadyStarted)
        ));
    }

    #[test]
    fn an_archived_class_takes_no_new_bookings_but_keeps_its_old_ones() {
        let mut class = monday_class(20);
        class.archive(now());
        assert!(matches!(
            ClassBooking::book(
                ClassBookingId::new(),
                &class,
                UserId::new(),
                date("2026-08-31"),
                0,
                date("2026-08-26"),
                at(9, 0),
                now(),
            ),
            Err(ClassError::Archived { .. })
        ));
    }

    #[test]
    fn archiving_is_idempotent_and_keeps_the_first_timestamp() {
        let mut class = monday_class(20);
        class.archive(now());
        let first = class.archived_at;
        class.archive(now() + chrono::Duration::days(1));
        assert_eq!(class.archived_at, first);
    }

    #[test]
    fn a_place_can_be_given_back_until_the_class_starts() {
        let class = monday_class(20);
        let monday = date("2026-08-31");
        let mut booking = ClassBooking::book(
            ClassBookingId::new(),
            &class,
            UserId::new(),
            monday,
            0,
            date("2026-08-26"),
            at(9, 0),
            now(),
        )
        .unwrap();

        assert!(booking.cancel(&class, monday, at(17, 30), now()).is_ok());
        assert!(!booking.is_live());
        // Twice is refused rather than silently accepted: the second caller
        // believes something untrue about the state.
        assert_eq!(
            booking.cancel(&class, monday, at(17, 30), now()),
            Err(ClassError::AlreadyCancelled)
        );
    }

    #[test]
    fn a_place_cannot_be_given_back_once_the_class_has_begun() {
        let class = monday_class(20);
        let monday = date("2026-08-31");
        let mut booking = ClassBooking::book(
            ClassBookingId::new(),
            &class,
            UserId::new(),
            monday,
            0,
            date("2026-08-26"),
            at(9, 0),
            now(),
        )
        .unwrap();
        assert_eq!(
            booking.cancel(&class, monday, at(18, 1), now()),
            Err(ClassError::AlreadyStarted)
        );
        assert!(booking.is_live(), "a refused cancel changes nothing");
    }
}

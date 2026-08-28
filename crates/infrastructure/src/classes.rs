//! Classes and bookings.
//!
//! The one interesting query here is `timetable`, which turns a weekly slot
//! into dated occurrences **in SQL** via `generate_series`. Doing it in Rust
//! would mean fetching classes, expanding them into dates, then issuing a
//! count query per occurrence — the N+1 the joined-count exists to avoid. One
//! statement returns the whole grid with occupancy and the caller's own
//! bookings already resolved.

use async_trait::async_trait;
use chrono::NaiveDate;
use gym_application::{
    ApplicationError, ApplicationResult,
    ports::{ClassOnDate, ClassRepository},
};
use gym_domain::{
    ClassBookingId, GymClassId, GymId, TenantContext, UserId,
    gym_class::{ClassBooking, GymClass},
};

use crate::db::{DbPool, begin_tenant_tx};

fn db_err(e: sqlx::Error) -> ApplicationError {
    ApplicationError::internal(e)
}

/// A second live place in the same sitting is the partial unique index firing.
/// It has to read as a conflict, not a 500: two people tapping Book at the same
/// instant is ordinary, and the loser should be told the class filled up.
fn map_booking(err: sqlx::Error) -> ApplicationError {
    if let sqlx::Error::Database(db) = &err
        && db.is_unique_violation()
    {
        return ApplicationError::StateConflict(
            "you already hold a place in that class".to_owned(),
        );
    }
    db_err(err)
}

#[derive(Debug, Clone)]
pub struct PgClassRepository {
    pool: DbPool,
}

impl PgClassRepository {
    #[must_use]
    pub const fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ClassRepository for PgClassRepository {
    async fn list_classes(&self, tenant: &TenantContext) -> ApplicationResult<Vec<GymClass>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let rows = sqlx::query!(
            r#"
            SELECT id, gym_id, name, instructor_id, weekday, starts_at,
                   duration_minutes, capacity, description, archived_at
            FROM gym_classes
            WHERE gym_id = $1 AND archived_at IS NULL
            ORDER BY weekday, starts_at, name
            "#,
            tenant.gym_id.into_uuid(),
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        Ok(rows
            .into_iter()
            .map(|r| GymClass {
                id: GymClassId::from(r.id),
                gym_id: GymId::from(r.gym_id),
                name: r.name,
                instructor_id: UserId::from(r.instructor_id),
                weekday: u8::try_from(r.weekday).unwrap_or(0),
                starts_at: r.starts_at,
                duration_minutes: u16::try_from(r.duration_minutes).unwrap_or(0),
                capacity: u32::try_from(r.capacity).unwrap_or(0),
                description: r.description,
                archived_at: r.archived_at,
            })
            .collect())
    }

    async fn find_class(
        &self,
        tenant: &TenantContext,
        id: GymClassId,
    ) -> ApplicationResult<Option<GymClass>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // Archived rows are returned, not filtered: cancelling a booking on a
        // class the gym has since dropped still has to find the class, and the
        // domain is what refuses a NEW booking against it.
        let row = sqlx::query!(
            r#"
            SELECT id, gym_id, name, instructor_id, weekday, starts_at,
                   duration_minutes, capacity, description, archived_at
            FROM gym_classes
            WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid(),
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        Ok(row.map(|r| GymClass {
            id: GymClassId::from(r.id),
            gym_id: GymId::from(r.gym_id),
            name: r.name,
            instructor_id: UserId::from(r.instructor_id),
            weekday: u8::try_from(r.weekday).unwrap_or(0),
            starts_at: r.starts_at,
            duration_minutes: u16::try_from(r.duration_minutes).unwrap_or(0),
            capacity: u32::try_from(r.capacity).unwrap_or(0),
            description: r.description,
            archived_at: r.archived_at,
        }))
    }

    async fn insert_class(&self, tenant: &TenantContext, class: &GymClass) -> ApplicationResult<()> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        sqlx::query!(
            r#"
            INSERT INTO gym_classes
                (id, gym_id, name, instructor_id, weekday, starts_at,
                 duration_minutes, capacity, description)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
            class.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            class.name,
            class.instructor_id.into_uuid(),
            i16::try_from(class.weekday).unwrap_or(0),
            class.starts_at,
            i16::try_from(class.duration_minutes).unwrap_or(0),
            i32::try_from(class.capacity).unwrap_or(0),
            class.description,
        )
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db) = &e
                && db.is_unique_violation()
            {
                return ApplicationError::StateConflict(
                    "a class of that name already runs at that time".to_owned(),
                );
            }
            db_err(e)
        })?;

        tx.commit().await.map_err(db_err)
    }

    async fn save_archived_class(
        &self,
        tenant: &TenantContext,
        class: &GymClass,
    ) -> ApplicationResult<()> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // Compare-and-swap on still-live, so two managers archiving at once
        // cannot both believe they were first.
        let done = sqlx::query!(
            r#"
            UPDATE gym_classes SET archived_at = $3
            WHERE gym_id = $1 AND id = $2 AND archived_at IS NULL
            "#,
            tenant.gym_id.into_uuid(),
            class.id.into_uuid(),
            class.archived_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        if done.rows_affected() == 0 {
            return Err(ApplicationError::StateConflict(
                "that class is already off the timetable".to_owned(),
            ));
        }

        tx.commit().await.map_err(db_err)
    }

    async fn timetable(
        &self,
        tenant: &TenantContext,
        from: NaiveDate,
        to: NaiveDate,
        for_member: UserId,
    ) -> ApplicationResult<Vec<ClassOnDate>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // generate_series walks every date in the window; the join keeps only
        // those falling on the class's weekday. EXTRACT(DOW) is 0=Sunday, which
        // is why the column uses that convention — no conversion anywhere.
        let rows = sqlx::query!(
            r#"
            SELECT c.id, c.gym_id, c.name, c.instructor_id, c.weekday, c.starts_at,
                   c.duration_minutes, c.capacity, c.description, c.archived_at,
                   u.display_name AS instructor_name,
                   d.day::date AS "on_date!",
                   (SELECT COUNT(*) FROM class_bookings b
                     WHERE b.class_id = c.id AND b.on_date = d.day::date
                       AND b.cancelled_at IS NULL) AS "booked!",
                   (SELECT b.id FROM class_bookings b
                     WHERE b.class_id = c.id AND b.on_date = d.day::date
                       AND b.member_id = $4 AND b.cancelled_at IS NULL
                     LIMIT 1) AS "my_booking_id?"
            FROM gym_classes c
            JOIN users u ON u.id = c.instructor_id
            CROSS JOIN generate_series($2::date, $3::date, interval '1 day') AS d(day)
            WHERE c.gym_id = $1
              AND c.archived_at IS NULL
              AND EXTRACT(DOW FROM d.day)::smallint = c.weekday
            ORDER BY d.day, c.starts_at, c.name
            "#,
            tenant.gym_id.into_uuid(),
            from,
            to,
            for_member.into_uuid(),
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        Ok(rows
            .into_iter()
            .map(|r| ClassOnDate {
                class: GymClass {
                    id: GymClassId::from(r.id),
                    gym_id: GymId::from(r.gym_id),
                    name: r.name,
                    instructor_id: UserId::from(r.instructor_id),
                    weekday: u8::try_from(r.weekday).unwrap_or(0),
                    starts_at: r.starts_at,
                    duration_minutes: u16::try_from(r.duration_minutes).unwrap_or(0),
                    capacity: u32::try_from(r.capacity).unwrap_or(0),
                    description: r.description,
                    archived_at: r.archived_at,
                },
                instructor_name: r.instructor_name,
                on_date: r.on_date,
                booked: u32::try_from(r.booked).unwrap_or(0),
                // Derived from the id rather than a second EXISTS: one source
                // of truth, so "booked" and "which booking" cannot disagree.
                booked_by_me: r.my_booking_id.is_some(),
                my_booking_id: r.my_booking_id.map(ClassBookingId::from),
            })
            .collect())
    }

    async fn live_booking_count(
        &self,
        tenant: &TenantContext,
        class: GymClassId,
        on_date: NaiveDate,
    ) -> ApplicationResult<u32> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let row = sqlx::query!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM class_bookings
            WHERE gym_id = $1 AND class_id = $2 AND on_date = $3
              AND cancelled_at IS NULL
            "#,
            tenant.gym_id.into_uuid(),
            class.into_uuid(),
            on_date,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(u32::try_from(row.count).unwrap_or(0))
    }

    async fn find_booking(
        &self,
        tenant: &TenantContext,
        id: ClassBookingId,
    ) -> ApplicationResult<Option<ClassBooking>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let row = sqlx::query!(
            r#"
            SELECT id, gym_id, class_id, member_id, on_date, cancelled_at, created_at
            FROM class_bookings
            WHERE gym_id = $1 AND id = $2
            "#,
            tenant.gym_id.into_uuid(),
            id.into_uuid(),
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        Ok(row.map(|r| ClassBooking {
            id: ClassBookingId::from(r.id),
            gym_id: GymId::from(r.gym_id),
            class_id: GymClassId::from(r.class_id),
            member_id: UserId::from(r.member_id),
            on_date: r.on_date,
            cancelled_at: r.cancelled_at,
            created_at: r.created_at,
        }))
    }

    async fn insert_booking(
        &self,
        tenant: &TenantContext,
        booking: &ClassBooking,
    ) -> ApplicationResult<()> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        // Client-minted id (ADR-0008): a replayed booking is the SAME booking,
        // so DO NOTHING rather than a conflict. The partial unique index is
        // what catches a genuine second place, and that one does conflict.
        sqlx::query!(
            r#"
            INSERT INTO class_bookings
                (id, gym_id, class_id, member_id, on_date, created_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (id) DO NOTHING
            "#,
            booking.id.into_uuid(),
            tenant.gym_id.into_uuid(),
            booking.class_id.into_uuid(),
            booking.member_id.into_uuid(),
            booking.on_date,
            booking.created_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_booking)?;

        tx.commit().await.map_err(db_err)
    }

    async fn save_cancelled_booking(
        &self,
        tenant: &TenantContext,
        booking: &ClassBooking,
    ) -> ApplicationResult<()> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let done = sqlx::query!(
            r#"
            UPDATE class_bookings SET cancelled_at = $3
            WHERE gym_id = $1 AND id = $2 AND cancelled_at IS NULL
            "#,
            tenant.gym_id.into_uuid(),
            booking.id.into_uuid(),
            booking.cancelled_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(db_err)?;

        if done.rows_affected() == 0 {
            return Err(ApplicationError::StateConflict(
                "that place was already given up".to_owned(),
            ));
        }

        tx.commit().await.map_err(db_err)
    }

    async fn roster(
        &self,
        tenant: &TenantContext,
        class: GymClassId,
        on_date: NaiveDate,
    ) -> ApplicationResult<Vec<(UserId, String)>> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let rows = sqlx::query!(
            r#"
            SELECT b.member_id, u.display_name
            FROM class_bookings b
            JOIN users u ON u.id = b.member_id
            WHERE b.gym_id = $1 AND b.class_id = $2 AND b.on_date = $3
              AND b.cancelled_at IS NULL
            ORDER BY u.display_name, b.member_id
            "#,
            tenant.gym_id.into_uuid(),
            class.into_uuid(),
            on_date,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;

        Ok(rows
            .into_iter()
            .map(|r| (UserId::from(r.member_id), r.display_name))
            .collect())
    }

    async fn timezone(&self, tenant: &TenantContext) -> ApplicationResult<String> {
        let mut tx = begin_tenant_tx(&self.pool, tenant).await.map_err(db_err)?;

        let row = sqlx::query!(
            r#"SELECT timezone FROM gyms WHERE id = $1"#,
            tenant.gym_id.into_uuid(),
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(db_err)?;

        tx.commit().await.map_err(db_err)?;
        Ok(row.map_or_else(|| "UTC".to_owned(), |r| r.timezone))
    }
}

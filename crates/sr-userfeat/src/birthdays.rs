//! FTR-BDAY-001 birthdays. A member sets month and day (the year only if they want to), and chooses whether
//! servers that run the module may show it. The daily pass announces today's birthdays in each such server's
//! channel and can grant a role for 24 hours. All dates are UTC. A 29 February birthday is celebrated on 28
//! February in years that have no 29th.

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::error::{UfError, UfResult, invalid};
use crate::{UserFeatures, features::BIRTHDAYS_ID};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BirthdayInput {
    /// 1 to 12.
    pub month: i16,
    /// 1 to 31, valid for the month.
    pub day: i16,
    /// Optional birth year (1900 to 2100); shared only as an age in announcements.
    #[serde(default)]
    pub year: Option<i16>,
    /// `servers` (default) lets servers that run the module show it; `hidden` keeps it private.
    #[serde(default)]
    pub visibility: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow, ToSchema)]
pub struct Birthday {
    pub month: i16,
    pub day: i16,
    pub year: Option<i16>,
    pub visibility: String,
}

/// A member's birthday as other members of a server see it (never the year).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow, ToSchema)]
pub struct UpcomingBirthday {
    pub user_id: String,
    pub month: i16,
    pub day: i16,
}

/// Someone to announce today.
#[derive(Debug, Clone, FromRow)]
pub struct Celebrant {
    pub user_id: i64,
    /// Age they turn today, when they shared their birth year.
    pub age: Option<i32>,
    pub name: String,
}

pub fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(month: i16, year: Option<i16>) -> i16 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        // Without a year 29 February is allowed.
        2 => match year {
            Some(y) if !is_leap(i32::from(y)) => 28,
            _ => 29,
        },
        _ => 0,
    }
}

/// Validate a birthday and fill the defaults.
pub fn check(input: &BirthdayInput) -> UfResult<(i16, i16, Option<i16>, &'static str)> {
    if !(1..=12).contains(&input.month) {
        return Err(invalid("month must be 1 to 12"));
    }
    if let Some(y) = input.year
        && !(1900..=2100).contains(&y)
    {
        return Err(invalid("year must be between 1900 and 2100"));
    }
    if input.day < 1 || input.day > days_in_month(input.month, input.year) {
        return Err(invalid("that day does not exist in that month"));
    }
    let visibility = match input.visibility.as_deref() {
        None | Some("servers") => "servers",
        Some("hidden") => "hidden",
        Some(_) => return Err(invalid("visibility must be `servers` or `hidden`")),
    };
    Ok((input.month, input.day, input.year, visibility))
}

impl UserFeatures {
    pub async fn set_birthday(&self, user: i64, input: &BirthdayInput) -> UfResult<Birthday> {
        let (month, day, year, visibility) = check(input)?;
        Ok(sqlx::query_as::<_, Birthday>(
            "INSERT INTO user_birthdays (user_id, month, day, year, visibility) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (user_id) DO UPDATE SET month = EXCLUDED.month, day = EXCLUDED.day, \
                 year = EXCLUDED.year, visibility = EXCLUDED.visibility, updated_at = now() \
             RETURNING month, day, year, visibility",
        )
        .bind(user)
        .bind(month)
        .bind(day)
        .bind(year)
        .bind(visibility)
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn get_birthday(&self, user: i64) -> UfResult<Birthday> {
        sqlx::query_as::<_, Birthday>(
            "SELECT month, day, year, visibility FROM user_birthdays WHERE user_id = $1",
        )
        .bind(user)
        .fetch_optional(&self.pool)
        .await?
        .ok_or(UfError::NotFound("birthday"))
    }

    pub async fn clear_birthday(&self, user: i64) -> UfResult<()> {
        sqlx::query("DELETE FROM user_birthdays WHERE user_id = $1")
            .bind(user)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The next birthdays of members who share theirs, soonest first (starting today).
    pub async fn upcoming_birthdays(
        &self,
        guild: i64,
        limit: i64,
    ) -> UfResult<Vec<UpcomingBirthday>> {
        self.require(BIRTHDAYS_ID, Some(guild)).await?;
        // Days until the next occurrence, counting 29 February as 28 February in a non-leap year is not
        // needed for ordering; the plain month/day order from today wraps around the year end.
        Ok(sqlx::query_as::<_, UpcomingBirthday>(
            "WITH t AS (SELECT (now() AT TIME ZONE 'UTC')::date AS d) \
             SELECT b.user_id::text AS user_id, b.month, b.day \
             FROM user_birthdays b JOIN guild_members m ON m.user_id = b.user_id AND m.guild_id = $1, t \
             WHERE b.visibility = 'servers' \
             ORDER BY ((b.month, b.day) < (extract(month FROM t.d)::int, extract(day FROM t.d)::int)), \
                      b.month, b.day, b.user_id \
             LIMIT $2",
        )
        .bind(guild)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await?)
    }

    /// Members of `guild` whose birthday is today (UTC) and who have not been announced yet today.
    pub async fn birthdays_today(&self, guild: i64) -> UfResult<Vec<Celebrant>> {
        Ok(sqlx::query_as::<_, Celebrant>(
            "WITH t AS (SELECT (now() AT TIME ZONE 'UTC')::date AS d) \
             SELECT b.user_id, \
                    CASE WHEN b.year IS NULL THEN NULL ELSE extract(year FROM t.d)::int - b.year END AS age, \
                    a.username::text AS name \
             FROM user_birthdays b \
             JOIN guild_members m ON m.user_id = b.user_id AND m.guild_id = $1 \
             JOIN accounts a ON a.id = b.user_id, t \
             WHERE b.visibility = 'servers' \
               AND ((b.month = extract(month FROM t.d)::int AND b.day = extract(day FROM t.d)::int) \
                    OR (b.month = 2 AND b.day = 29 AND extract(month FROM t.d) = 2 AND extract(day FROM t.d) = 28 \
                        AND NOT (extract(year FROM t.d)::int % 4 = 0 \
                                 AND (extract(year FROM t.d)::int % 100 <> 0 OR extract(year FROM t.d)::int % 400 = 0)))) \
               AND NOT EXISTS (SELECT 1 FROM birthday_announcements x \
                               WHERE x.guild_id = $1 AND x.user_id = b.user_id AND x.on_date = t.d) \
             ORDER BY b.user_id",
        )
        .bind(guild)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Mark `user` announced in `guild` today. `false` when someone else already did (two nodes racing).
    pub async fn mark_announced(&self, guild: i64, user: i64, role: Option<i64>) -> UfResult<bool> {
        Ok(sqlx::query(
            "INSERT INTO birthday_announcements (guild_id, user_id, on_date, role_id, role_until) \
             VALUES ($1, $2, (now() AT TIME ZONE 'UTC')::date, $3, \
                     CASE WHEN $3::bigint IS NULL THEN NULL ELSE now() + interval '24 hours' END) \
             ON CONFLICT DO NOTHING",
        )
        .bind(guild)
        .bind(user)
        .bind(role)
        .execute(&self.pool)
        .await?
        .rows_affected()
            == 1)
    }

    /// Birthday roles whose 24 hours are over: `(guild, user, role)`.
    pub async fn expired_birthday_roles(&self, limit: i64) -> UfResult<Vec<(i64, i64, i64)>> {
        Ok(sqlx::query_as::<_, (i64, i64, i64)>(
            "SELECT guild_id, user_id, role_id FROM birthday_announcements \
             WHERE role_id IS NOT NULL AND role_until <= now() ORDER BY role_until LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?)
    }

    /// Forget today's announcement (the post failed, so the next pass tries again).
    pub async fn unmark_announced(&self, guild: i64, user: i64) -> UfResult<()> {
        sqlx::query(
            "DELETE FROM birthday_announcements              WHERE guild_id = $1 AND user_id = $2 AND on_date = (now() AT TIME ZONE 'UTC')::date",
        )
        .bind(guild)
        .bind(user)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn clear_birthday_role(&self, guild: i64, user: i64, role: i64) -> UfResult<()> {
        sqlx::query(
            "UPDATE birthday_announcements SET role_id = NULL, role_until = NULL \
             WHERE guild_id = $1 AND user_id = $2 AND role_id = $3",
        )
        .bind(guild)
        .bind(user)
        .bind(role)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(month: i16, day: i16, year: Option<i16>) -> BirthdayInput {
        BirthdayInput {
            month,
            day,
            year,
            visibility: None,
        }
    }

    #[test]
    fn validates_calendar_days() {
        assert!(check(&input(2, 29, None)).is_ok());
        assert!(check(&input(2, 29, Some(2000))).is_ok());
        assert!(check(&input(2, 29, Some(2001))).is_err());
        assert!(check(&input(2, 29, Some(1900))).is_err());
        assert!(check(&input(4, 31, None)).is_err());
        assert!(check(&input(13, 1, None)).is_err());
        assert!(check(&input(1, 0, None)).is_err());
        assert!(check(&input(1, 1, Some(1850))).is_err());
        let mut v = input(5, 5, None);
        v.visibility = Some("friends".into());
        assert!(check(&v).is_err());
        v.visibility = Some("hidden".into());
        assert_eq!(check(&v).unwrap().3, "hidden");
    }

    #[test]
    fn leap_years() {
        assert!(is_leap(2024));
        assert!(!is_leap(2100));
        assert!(is_leap(2000));
    }
}

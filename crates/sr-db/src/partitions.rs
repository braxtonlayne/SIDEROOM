//! Monthly range-partition manager (FND-DB-007).
//!
//! Tables such as `messages` are range-partitioned by Snowflake `id`, which is
//! time-ordered, so one partition covers one calendar month (UTC): the lower
//! bound is the smallest id any worker could mint at the start of the month.
//! The helper is generic over the table name because `messages` is not in the
//! migrations yet; the parent must be declared `PARTITION BY RANGE (id)`.
//!
//! Intended to run as a job of kind [`JOB_KIND`] (create-ahead), and it is
//! idempotent (`CREATE TABLE IF NOT EXISTS`).

use sqlx::PgPool;
use sr_core::snowflake::SIDEROOM_EPOCH_MS;

use crate::jobs::Job;
use crate::{DbError, DbResult};

/// Job kind for the create-ahead job. Payload: `{"table": "messages", "months_ahead": 3}`.
pub const JOB_KIND: &str = "db.partitions.ensure";

/// Default number of future months to keep created.
pub const DEFAULT_MONTHS_AHEAD: u32 = 3;

/// Bits below the timestamp in a Snowflake (worker 5 + process 5 + sequence 12).
const TS_SHIFT: u32 = 22;

/// A calendar month in UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Month {
    pub year: i32,
    /// 1..=12
    pub month: u32,
}

impl Month {
    /// The month containing `unix_ms`.
    pub fn containing(unix_ms: i64) -> Self {
        let days = unix_ms.div_euclid(86_400_000);
        let (year, month, _) = civil_from_days(days);
        Self { year, month }
    }

    /// The month `n` months after this one.
    pub fn plus_months(self, n: u32) -> Self {
        let idx = i64::from(self.year) * 12 + i64::from(self.month - 1) + i64::from(n);
        Self {
            year: i32::try_from(idx.div_euclid(12)).unwrap_or(i32::MAX),
            month: u32::try_from(idx.rem_euclid(12)).unwrap_or(0) + 1,
        }
    }

    /// First instant of the month, Unix ms.
    pub fn start_unix_ms(self) -> i64 {
        days_from_civil(self.year, self.month, 1) * 86_400_000
    }

    /// Smallest Snowflake id belonging to this month (clamped at 0 before the epoch).
    pub fn lower_bound_id(self) -> i64 {
        (self.start_unix_ms() - SIDEROOM_EPOCH_MS).max(0) << TS_SHIFT
    }
}

// Howard Hinnant's civil-date algorithms.
fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    let y = i64::from(y) - i64::from(m <= 2);
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (
        i32::try_from(y + i64::from(m <= 2)).unwrap_or(i32::MAX),
        m,
        d,
    )
}

fn check_ident(table: &str) -> DbResult<()> {
    let ok = !table.is_empty()
        && table.len() <= 48
        && table
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        && !table.as_bytes()[0].is_ascii_digit();
    if ok {
        Ok(())
    } else {
        Err(DbError::InvalidIdentifier(table.to_owned()))
    }
}

/// Partition table name, e.g. `messages_2026_03`.
pub fn partition_name(table: &str, m: Month) -> String {
    format!("{table}_{:04}_{:02}", m.year, m.month)
}

/// The DDL creating the partition for month `m`.
pub fn partition_sql(table: &str, m: Month) -> DbResult<String> {
    check_ident(table)?;
    Ok(format!(
        "CREATE TABLE IF NOT EXISTS {} PARTITION OF {table} FOR VALUES FROM ({}) TO ({})",
        partition_name(table, m),
        m.lower_bound_id(),
        m.plus_months(1).lower_bound_id()
    ))
}

/// Ensure partitions exist for the month of `now_unix_ms` and `months_ahead`
/// following months. Returns the partition names ensured. Idempotent.
pub async fn ensure_ahead(
    pool: &PgPool,
    table: &str,
    now_unix_ms: i64,
    months_ahead: u32,
) -> DbResult<Vec<String>> {
    check_ident(table)?;
    let first = Month::containing(now_unix_ms);
    let mut out = Vec::new();
    for i in 0..=months_ahead {
        let m = first.plus_months(i);
        sqlx::query(sqlx::AssertSqlSafe(partition_sql(table, m)?))
            .execute(pool)
            .await?;
        out.push(partition_name(table, m));
    }
    Ok(out)
}

/// Job handler body for [`JOB_KIND`].
pub async fn run_job(pool: &PgPool, job: &Job, now_unix_ms: i64) -> DbResult<Vec<String>> {
    let table = job
        .payload
        .get("table")
        .and_then(|v| v.as_str())
        .ok_or_else(|| DbError::InvalidIdentifier("<missing table>".into()))?;
    let ahead = job
        .payload
        .get("months_ahead")
        .and_then(|v| v.as_u64())
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(DEFAULT_MONTHS_AHEAD);
    ensure_ahead(pool, table, now_unix_ms, ahead).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_math() {
        // 2026-01-01T00:00:00Z is the Snowflake epoch.
        let jan = Month::containing(SIDEROOM_EPOCH_MS);
        assert_eq!(
            jan,
            Month {
                year: 2026,
                month: 1
            }
        );
        assert_eq!(jan.start_unix_ms(), SIDEROOM_EPOCH_MS);
        assert_eq!(jan.lower_bound_id(), 0);
        assert_eq!(
            jan.plus_months(12),
            Month {
                year: 2027,
                month: 1
            }
        );
        let dec = Month {
            year: 2026,
            month: 12,
        };
        assert_eq!(
            dec.plus_months(1),
            Month {
                year: 2027,
                month: 1
            }
        );
        // 2028 is a leap year: February has 29 days.
        let feb = Month {
            year: 2028,
            month: 2,
        };
        assert_eq!(
            feb.plus_months(1).start_unix_ms() - feb.start_unix_ms(),
            29 * 86_400_000
        );
        // Last ms of December stays in December.
        assert_eq!(
            Month::containing(dec.plus_months(1).start_unix_ms() - 1).month,
            12
        );
    }

    #[test]
    fn bounds_are_increasing() {
        let m = Month {
            year: 2026,
            month: 5,
        };
        assert!(m.lower_bound_id() > 0);
        assert!(m.plus_months(1).lower_bound_id() > m.lower_bound_id());
    }

    #[test]
    fn sql_and_ident_validation() {
        let m = Month {
            year: 2026,
            month: 3,
        };
        let sql = partition_sql("messages", m).unwrap();
        assert!(
            sql.starts_with("CREATE TABLE IF NOT EXISTS messages_2026_03 PARTITION OF messages")
        );
        assert!(partition_sql("messages; DROP TABLE x", m).is_err());
        assert!(partition_sql("Messages", m).is_err());
        assert!(partition_sql("", m).is_err());
        assert!(partition_sql("1abc", m).is_err());
    }
}

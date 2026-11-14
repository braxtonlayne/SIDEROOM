//! The two server limits an operator sets in Admin > Settings (D-659, D-660, D-661).
//!
//! * [`GUILDS_PER_USER_SETTING`]: the most servers one account may be in (created or joined). The Staff Server
//!   is not counted; bots and the system user are not limited.
//! * [`GUILD_MEMBERS_SETTING`]: the most members one server may have (people, pending members and bots). The
//!   Staff Server has no limit.
//!
//! A stored value that is missing, not a whole number or out of range gives the default, never zero.

use serde_json::Value;

/// Setting key: the most servers one account may be in.
pub const GUILDS_PER_USER_SETTING: &str = "guilds.max_per_user";
/// Default of [`GUILDS_PER_USER_SETTING`].
pub const DEFAULT_GUILDS_PER_USER: i64 = 200;
/// Allowed range of [`GUILDS_PER_USER_SETTING`].
pub const GUILDS_PER_USER_RANGE: std::ops::RangeInclusive<i64> = 1..=1_000;

/// Setting key: the most members one server may have.
pub const GUILD_MEMBERS_SETTING: &str = "guilds.max_members";
/// Default of [`GUILD_MEMBERS_SETTING`].
pub const DEFAULT_GUILD_MEMBERS: i64 = 25_000;
/// Allowed range of [`GUILD_MEMBERS_SETTING`].
pub const GUILD_MEMBERS_RANGE: std::ops::RangeInclusive<i64> = 2..=1_000_000;

/// The number in force for a stored setting value: `stored` when it is a whole number inside `range`,
/// otherwise `default`.
#[must_use]
pub fn limit_in_force(
    stored: Option<&Value>,
    range: &std::ops::RangeInclusive<i64>,
    default: i64,
) -> i64 {
    match stored.and_then(Value::as_i64) {
        Some(n) if range.contains(&n) => n,
        _ => default,
    }
}

/// The servers-per-account limit in force for a stored value.
#[must_use]
pub fn guilds_per_user_in_force(stored: Option<&Value>) -> i64 {
    limit_in_force(stored, &GUILDS_PER_USER_RANGE, DEFAULT_GUILDS_PER_USER)
}

/// The members-per-server limit in force for a stored value.
#[must_use]
pub fn guild_members_in_force(stored: Option<&Value>) -> i64 {
    limit_in_force(stored, &GUILD_MEMBERS_RANGE, DEFAULT_GUILD_MEMBERS)
}

/// Which limit an action ran into, and the number in force.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapHit {
    /// The account is in the most servers it may be in (the number is the limit in force).
    UserServers(i64),
    /// The server has the most members it may have.
    GuildMembers(i64),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn unreadable_values_give_the_default() {
        for v in [
            json!("many"),
            json!(0),
            json!(-1),
            json!(1001),
            json!(2.5),
            json!(null),
        ] {
            assert_eq!(guilds_per_user_in_force(Some(&v)), 200, "{v}");
        }
        assert_eq!(guilds_per_user_in_force(None), 200);
        assert_eq!(guilds_per_user_in_force(Some(&json!(3))), 3);
        assert_eq!(guild_members_in_force(Some(&json!(1))), 25_000);
        assert_eq!(guild_members_in_force(Some(&json!(2))), 2);
        assert_eq!(guild_members_in_force(Some(&json!(1_000_001))), 25_000);
    }
}

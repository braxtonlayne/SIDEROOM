//! FTR-LVL-001 leveling (opt-in). Members earn XP for chatting, at most once per cooldown, and rise through
//! levels on the same curve as the popular bots (the XP to go from level `n` to `n + 1` is `5n^2 + 50n + 100`).
//! A server turns the module on (it is off by default) and members can opt out in that server, which also
//! deletes their XP.

use serde::Serialize;
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::config::Settings;
use crate::error::{UfError, UfResult};
use crate::{UserFeatures, features::LEVELING_ID};

/// XP needed to go from `level` to `level + 1`.
pub fn xp_to_next(level: i64) -> i64 {
    5 * level * level + 50 * level + 100
}

/// Total XP at which `level` begins.
pub fn xp_for_level(level: i64) -> i64 {
    (0..level.max(0)).map(xp_to_next).sum()
}

/// The level a total of `xp` reaches.
pub fn level_for_xp(xp: i64) -> i64 {
    let mut level = 0;
    let mut left = xp.max(0);
    while left >= xp_to_next(level) {
        left -= xp_to_next(level);
        level += 1;
    }
    level
}

/// An XP amount in `min..=max` chosen by `seed` (any varying number: a clock, an id).
pub fn pick_xp(min: i64, max: i64, seed: u64) -> i64 {
    let (lo, hi) = if min <= max { (min, max) } else { (max, min) };
    let span = u64::try_from(hi - lo + 1).unwrap_or(1).max(1);
    // Mix the seed so consecutive ids do not give consecutive amounts.
    let mixed = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .rotate_left(29)
        .wrapping_mul(0xBF58_476D_1CE4_E5B9);
    lo + i64::try_from(mixed % span).unwrap_or(0)
}

/// The roles a server hands out for reaching `new` coming from `old`.
pub fn roles_between(cfg: &Settings, old: i64, new: i64) -> Vec<i64> {
    cfg.list("level_roles")
        .into_iter()
        .filter_map(|v| {
            let level = v.get("level")?.as_i64()?;
            let role = v.get("role")?.as_str()?.parse::<i64>().ok()?;
            (level > old && level <= new && role > 0).then_some(role)
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelUp {
    pub old: i64,
    pub new: i64,
    pub xp: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct RankCard {
    pub user_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub xp: i64,
    pub level: i64,
    /// XP earned inside the current level.
    pub level_xp: i64,
    /// XP the current level spans (progress is `level_xp / level_span`).
    pub level_span: i64,
    /// 1 for the member with the most XP; absent for a member with no XP yet.
    pub rank: Option<i64>,
    pub messages: i64,
    /// Whether the member opted out of leveling in this server.
    pub opted_out: bool,
}

#[derive(Debug, Clone, FromRow)]
struct RankRow {
    xp: i64,
    messages: i64,
}

impl UserFeatures {
    /// Give `user` `amount` XP in `guild` unless they are on cooldown or opted out. `None` when nothing was
    /// awarded or the level did not change.
    pub async fn award_xp(
        &self,
        guild: i64,
        user: i64,
        amount: i64,
        cooldown_secs: i64,
    ) -> UfResult<Option<LevelUp>> {
        let row = sqlx::query_as::<_, (i64, i32)>(
            "INSERT INTO member_xp (guild_id, user_id, xp, level, messages, last_award_at) \
             SELECT $1, $2, $3, 0, 1, now() \
             WHERE NOT EXISTS (SELECT 1 FROM member_xp_optout o WHERE o.guild_id = $1 AND o.user_id = $2) \
             ON CONFLICT (guild_id, user_id) DO UPDATE \
               SET xp = member_xp.xp + EXCLUDED.xp, messages = member_xp.messages + 1, last_award_at = now() \
               WHERE member_xp.last_award_at IS NULL \
                  OR member_xp.last_award_at <= now() - make_interval(secs => $4::double precision) \
             RETURNING xp, level",
        )
        .bind(guild)
        .bind(user)
        .bind(amount)
        .bind(cooldown_secs.max(0))
        .fetch_optional(&self.pool)
        .await?;
        let Some((xp, old)) = row else {
            return Ok(None);
        };
        let new = level_for_xp(xp);
        if new == i64::from(old) {
            return Ok(None);
        }
        sqlx::query("UPDATE member_xp SET level = $3 WHERE guild_id = $1 AND user_id = $2")
            .bind(guild)
            .bind(user)
            .bind(i32::try_from(new).unwrap_or(i32::MAX))
            .execute(&self.pool)
            .await?;
        Ok(Some(LevelUp {
            old: i64::from(old),
            new,
            xp,
        }))
    }

    pub async fn rank_card(&self, guild: i64, user: i64) -> UfResult<RankCard> {
        self.require(LEVELING_ID, Some(guild)).await?;
        let who: Option<(String, Option<String>)> =
            sqlx::query_as("SELECT username::text, display_name FROM accounts WHERE id = $1")
                .bind(user)
                .fetch_optional(&self.pool)
                .await?;
        let (username, display_name) = who.ok_or(UfError::NotFound("member"))?;
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM guild_members WHERE guild_id = $1 AND user_id = $2)",
        )
        .bind(guild)
        .bind(user)
        .fetch_one(&self.pool)
        .await?;
        if !member {
            return Err(UfError::NotFound("member"));
        }
        let opted_out: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM member_xp_optout WHERE guild_id = $1 AND user_id = $2)",
        )
        .bind(guild)
        .bind(user)
        .fetch_one(&self.pool)
        .await?;
        let row = sqlx::query_as::<_, RankRow>(
            "SELECT xp, messages FROM member_xp WHERE guild_id = $1 AND user_id = $2",
        )
        .bind(guild)
        .bind(user)
        .fetch_optional(&self.pool)
        .await?;
        let (xp, messages, rank) = match row {
            Some(r) => {
                let ahead: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM member_xp WHERE guild_id = $1 \
                     AND (xp > $2 OR (xp = $2 AND user_id < $3))",
                )
                .bind(guild)
                .bind(r.xp)
                .bind(user)
                .fetch_one(&self.pool)
                .await?;
                (r.xp, r.messages, Some(ahead + 1))
            }
            None => (0, 0, None),
        };
        let level = level_for_xp(xp);
        Ok(RankCard {
            user_id: user.to_string(),
            username,
            display_name,
            xp,
            level,
            level_xp: xp - xp_for_level(level),
            level_span: xp_to_next(level),
            rank,
            messages,
            opted_out,
        })
    }

    /// The top of a server's board, `offset` rows in.
    pub async fn leaderboard(
        &self,
        guild: i64,
        limit: i64,
        offset: i64,
    ) -> UfResult<Vec<LeaderboardEntry>> {
        self.require(LEVELING_ID, Some(guild)).await?;
        Ok(sqlx::query_as::<_, LeaderboardEntry>(
            "SELECT (row_number() OVER (ORDER BY x.xp DESC, x.user_id) + $3)::bigint AS rank, \
                    x.user_id::text AS user_id, a.username::text AS username, a.display_name, \
                    a.avatar_key, x.xp, x.level::bigint AS level \
             FROM (SELECT * FROM member_xp WHERE guild_id = $1 ORDER BY xp DESC, user_id LIMIT $2 OFFSET $3) x \
             JOIN accounts a ON a.id = x.user_id \
             ORDER BY x.xp DESC, x.user_id",
        )
        .bind(guild)
        .bind(limit.clamp(1, 100))
        .bind(offset.max(0))
        .fetch_all(&self.pool)
        .await?)
    }

    /// Opt in or out of leveling in a server. Opting out deletes the member's XP.
    pub async fn set_xp_opt_out(&self, guild: i64, user: i64, out: bool) -> UfResult<()> {
        if out {
            let mut tx = self.pool.begin().await?;
            sqlx::query(
                "INSERT INTO member_xp_optout (guild_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(guild)
            .bind(user)
            .execute(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM member_xp WHERE guild_id = $1 AND user_id = $2")
                .bind(guild)
                .bind(user)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
        } else {
            sqlx::query("DELETE FROM member_xp_optout WHERE guild_id = $1 AND user_id = $2")
                .bind(guild)
                .bind(user)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, FromRow, ToSchema)]
pub struct LeaderboardEntry {
    pub rank: i64,
    pub user_id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_key: Option<String>,
    pub xp: i64,
    pub level: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn curve_matches_the_known_table() {
        assert_eq!(xp_to_next(0), 100);
        assert_eq!(xp_to_next(1), 155);
        assert_eq!(xp_to_next(2), 220);
        assert_eq!(xp_for_level(0), 0);
        assert_eq!(xp_for_level(3), 100 + 155 + 220);
    }

    #[test]
    fn level_for_xp_is_the_inverse() {
        for level in 0..60 {
            let at = xp_for_level(level);
            assert_eq!(level_for_xp(at), level);
            assert_eq!(level_for_xp(at + xp_to_next(level) - 1), level);
            assert_eq!(level_for_xp(at + xp_to_next(level)), level + 1);
        }
        assert_eq!(level_for_xp(-5), 0);
    }

    #[test]
    fn picked_xp_stays_in_range_and_varies() {
        let mut seen = std::collections::BTreeSet::new();
        for seed in 0..500u64 {
            let v = pick_xp(15, 25, seed);
            assert!((15..=25).contains(&v));
            seen.insert(v);
        }
        assert!(seen.len() >= 8);
        assert_eq!(pick_xp(7, 7, 99), 7);
        // Reversed bounds are tolerated.
        assert!((3..=5).contains(&pick_xp(5, 3, 1)));
    }

    #[test]
    fn level_roles_are_picked_for_the_levels_crossed() {
        let cfg = Settings(
            json!({"level_roles": [
                {"level": 5, "role": "100"},
                {"level": 10, "role": "200"},
                {"level": 15, "role": "300"},
                {"level": 20, "role": "bad"}
            ]})
            .as_object()
            .cloned()
            .unwrap_or_default(),
        );
        assert_eq!(roles_between(&cfg, 4, 5), vec![100]);
        assert_eq!(roles_between(&cfg, 0, 12), vec![100, 200]);
        assert!(roles_between(&cfg, 5, 9).is_empty());
        assert!(roles_between(&cfg, 15, 25).is_empty());
    }
}

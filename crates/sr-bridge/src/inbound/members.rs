//! Discord members as bridged members (owner direction on BRG-GW-001): parsing, the role cache, and the table.
//!
//! They are not accounts. Nothing here creates a user, a session, a DM or a permission; the table is read only
//! by the member list route of the bridged channel.

use std::collections::HashMap;
use std::sync::Mutex;

use serde_json::Value;
use sqlx::PgPool;

use super::convert::{clean_name, id_of};
use crate::error::BridgeResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub name: String,
    pub color: i32,
    pub position: i64,
}

/// One member as the gateway sends it (`GUILD_MEMBER_ADD`/`UPDATE`, an entry of a members chunk).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InMember {
    pub user_id: i64,
    pub name: String,
    pub avatar: Option<String>,
    pub bot: bool,
    pub role_ids: Vec<i64>,
}

pub fn parse_member(v: &Value) -> Option<InMember> {
    let user = v.get("user")?;
    let user_id = id_of(user.get("id")?)?;
    let name = v
        .get("nick")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            user.get("global_name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
        })
        .or_else(|| user.get("username").and_then(Value::as_str))
        .unwrap_or("Discord user");
    let hash = user
        .get("avatar")
        .and_then(Value::as_str)
        .filter(|h| safe_hash(h));
    Some(InMember {
        user_id,
        name: clean_name(name),
        avatar: hash
            .map(|h| format!("https://cdn.discordapp.com/avatars/{user_id}/{h}.png?size=128")),
        bot: user.get("bot").and_then(Value::as_bool).unwrap_or(false),
        role_ids: v
            .get("roles")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(id_of).collect())
            .unwrap_or_default(),
    })
}

fn safe_hash(h: &str) -> bool {
    !h.is_empty() && h.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn parse_role(v: &Value) -> Option<(i64, Role)> {
    Some((
        id_of(v.get("id")?)?,
        Role {
            name: clean_name(v.get("name").and_then(Value::as_str).unwrap_or("")),
            color: i32::try_from(v.get("color").and_then(Value::as_i64).unwrap_or(0)).unwrap_or(0),
            position: v.get("position").and_then(Value::as_i64).unwrap_or(0),
        },
    ))
}

pub fn parse_roles(v: &Value) -> Vec<(i64, Role)> {
    v.as_array()
        .map(|a| a.iter().filter_map(parse_role).collect())
        .unwrap_or_default()
}

/// `online`, `idle`, `dnd`; anything else (including `invisible` and `offline`) is `offline`.
pub fn presence_of(status: &str) -> &'static str {
    match status {
        "online" => "online",
        "idle" => "idle",
        "dnd" => "dnd",
        _ => "offline",
    }
}

/// The highest role (by position) among `ids`, never `@everyone` (whose id is the guild's).
pub fn top_role<'a>(
    roles: &'a HashMap<i64, Role>,
    ids: &[i64],
    guild: i64,
) -> Option<(i64, &'a Role)> {
    ids.iter()
        .filter(|id| **id != guild)
        .filter_map(|id| roles.get(id).map(|r| (*id, r)))
        .max_by_key(|(_, r)| r.position)
}

/// The roles of each Discord server the bridges are in, kept from `GUILD_CREATE` and `GUILD_ROLE_*`.
#[derive(Default)]
pub struct RoleCache {
    by_guild: Mutex<HashMap<i64, HashMap<i64, Role>>>,
}

impl RoleCache {
    pub fn replace(&self, guild: i64, roles: Vec<(i64, Role)>) {
        self.lock().insert(guild, roles.into_iter().collect());
    }
    pub fn set(&self, guild: i64, id: i64, role: Role) {
        self.lock().entry(guild).or_default().insert(id, role);
    }
    pub fn remove(&self, guild: i64, id: i64) {
        if let Some(m) = self.lock().get_mut(&guild) {
            m.remove(&id);
        }
    }
    /// `(role id, name, colour)` of the member's top role.
    pub fn top(&self, guild: i64, ids: &[i64]) -> Option<(i64, String, i32)> {
        let g = self.lock();
        let roles = g.get(&guild)?;
        top_role(roles, ids, guild).map(|(id, r)| (id, r.name.clone(), r.color))
    }
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<i64, HashMap<i64, Role>>> {
        self.by_guild
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

pub async fn upsert(
    pool: &PgPool,
    guild: i64,
    m: &InMember,
    role: Option<(i64, String, i32)>,
) -> BridgeResult<()> {
    let (rid, rname, rcolor) = match role {
        Some((i, n, c)) => (Some(i), Some(n), Some(c)),
        None => (None, None, None),
    };
    sqlx::query(
        "INSERT INTO bridge_members \
           (guild_id, discord_user_id, display_name, avatar_url, role_id, role_name, role_color, is_bot) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (guild_id, discord_user_id) DO UPDATE SET \
           display_name = EXCLUDED.display_name, avatar_url = EXCLUDED.avatar_url, \
           role_id = EXCLUDED.role_id, role_name = EXCLUDED.role_name, role_color = EXCLUDED.role_color, \
           is_bot = EXCLUDED.is_bot, updated_at = now()",
    )
    .bind(guild)
    .bind(m.user_id)
    .bind(&m.name)
    .bind(&m.avatar)
    .bind(rid)
    .bind(rname)
    .bind(rcolor)
    .bind(m.bot)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove(pool: &PgPool, guild: i64, user: i64) -> BridgeResult<bool> {
    Ok(
        sqlx::query("DELETE FROM bridge_members WHERE guild_id = $1 AND discord_user_id = $2")
            .bind(guild)
            .bind(user)
            .execute(pool)
            .await?
            .rows_affected()
            > 0,
    )
}

pub async fn set_presence(pool: &PgPool, guild: i64, user: i64, status: &str) -> BridgeResult<()> {
    sqlx::query(
        "UPDATE bridge_members SET presence = $3 WHERE guild_id = $1 AND discord_user_id = $2",
    )
    .bind(guild)
    .bind(user)
    .bind(presence_of(status))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_role(
    pool: &PgPool,
    guild: i64,
    role: i64,
    name: &str,
    color: i32,
) -> BridgeResult<()> {
    sqlx::query(
        "UPDATE bridge_members SET role_name = $3, role_color = $4 WHERE guild_id = $1 AND role_id = $2",
    )
    .bind(guild)
    .bind(role)
    .bind(name)
    .bind(color)
    .execute(pool)
    .await?;
    Ok(())
}

/// A full sync starts: everyone is stale until the chunks mention them again.
pub async fn mark_all_stale(pool: &PgPool, guild: i64) -> BridgeResult<()> {
    sqlx::query("UPDATE bridge_members SET updated_at = 'epoch' WHERE guild_id = $1")
        .bind(guild)
        .execute(pool)
        .await?;
    Ok(())
}

/// The last chunk arrived: whoever was not mentioned has left.
pub async fn drop_stale(pool: &PgPool, guild: i64) -> BridgeResult<u64> {
    Ok(
        sqlx::query("DELETE FROM bridge_members WHERE guild_id = $1 AND updated_at = 'epoch'")
            .bind(guild)
            .execute(pool)
            .await?
            .rows_affected(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn role(name: &str, color: i32, position: i64) -> Role {
        Role {
            name: name.into(),
            color,
            position,
        }
    }

    #[test]
    fn a_member_takes_the_nick_then_the_display_name_and_a_cdn_avatar() {
        let m = parse_member(&json!({"nick": "Bobby", "roles": ["5", "6"],
            "user": {"id": "7", "username": "bob", "global_name": "Bob", "avatar": "abc", "bot": true}}))
        .unwrap();
        assert_eq!(m.name, "Bobby");
        assert!(m.bot);
        assert_eq!(m.role_ids, [5, 6]);
        assert!(m.avatar.unwrap().ends_with("/avatars/7/abc.png?size=128"));
        let n = parse_member(&json!({"user": {"id": "7", "username": "bob"}})).unwrap();
        assert_eq!((n.name.as_str(), n.avatar), ("bob", None));
    }

    #[test]
    fn the_top_role_is_the_highest_position_and_never_everyone() {
        let roles: HashMap<i64, Role> = [
            (1, role("everyone", 0, 0)),
            (5, role("Mod", 0xff0000, 3)),
            (6, role("Fan", 0x00ff00, 1)),
        ]
        .into();
        assert_eq!(top_role(&roles, &[1, 6, 5], 1).unwrap().1.name, "Mod");
        assert_eq!(top_role(&roles, &[1], 1), None);
        assert_eq!(top_role(&roles, &[99], 1), None, "an unknown role is none");
    }

    #[test]
    fn presence_is_one_of_four() {
        assert_eq!(presence_of("online"), "online");
        assert_eq!(presence_of("dnd"), "dnd");
        assert_eq!(presence_of("invisible"), "offline");
        assert_eq!(presence_of("whatever"), "offline");
    }
}

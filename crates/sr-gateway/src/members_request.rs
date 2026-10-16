//! Op 8 `REQUEST_GUILD_MEMBERS` (D-242), Discord's contract: `guild_id`, `query` xor `user_ids`
//! (at most 100), `limit`, `presences`, `nonce` (at most 32 bytes). The answer is
//! `GUILD_MEMBERS_CHUNK` dispatches of at most [`CHUNK_SIZE`] members with `chunk_index`,
//! `chunk_count`, `not_found` and the echoed `nonce`.
//!
//! A malformed payload closes the connection with 4002. Asking for everyone (the empty query)
//! needs the `GUILD_MEMBERS` intent and `presences: true` the `GUILD_PRESENCES` intent, else the
//! close code is 4014; user sessions never get the whole list. The caller must be a member of
//! the guild (a bot: have it installed); a request for another guild is ignored. Member objects
//! come from the directory (the REST search route's repository) and never carry `sr_join` or
//! `sr_signals`. The request is one inbound op against the connection's existing budget.

use std::sync::Arc;

use serde_json::{Map, Value, json};
use sr_core::{GuildId, UserId};

use crate::hub::{Gateway, friend_presence_body};
use crate::protocol::{close, intents};
use crate::session::Session;
use crate::traits::MemberRequest;

/// Most members in one `GUILD_MEMBERS_CHUNK`.
pub const CHUNK_SIZE: usize = 1000;
/// Most ids one request may name.
pub const MAX_USER_IDS: usize = 100;
/// Longest nonce, in bytes.
pub const MAX_NONCE_BYTES: usize = 32;
/// What a non-empty query with `limit` 0 returns at most.
pub(crate) const DEFAULT_QUERY_LIMIT: usize = 100;

/// A validated op 8 payload.
#[derive(Debug, PartialEq)]
pub(crate) struct Parsed {
    pub guild: GuildId,
    pub request: MemberRequest,
    /// Ids that did not parse as snowflakes; reported as `not_found`.
    pub bad_ids: Vec<String>,
    pub presences: bool,
    pub nonce: Option<String>,
}

fn snowflake(v: &Value) -> Option<u64> {
    match v {
        Value::String(s) => s.parse().ok(),
        Value::Number(n) => n.as_u64(),
        _ => None,
    }
}

/// Parse the payload; `Err` is the close code (4002).
pub(crate) fn parse(d: &Value) -> Result<Parsed, u16> {
    let bad = close::DECODE_ERROR;
    let o: &Map<String, Value> = d.as_object().ok_or(bad)?;
    let guild = o
        .get("guild_id")
        .and_then(snowflake)
        .map(GuildId::new)
        .ok_or(bad)?;
    let presences = match o.get("presences") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(bad),
    };
    let nonce = match o.get("nonce") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.len() <= MAX_NONCE_BYTES => Some(s.clone()),
        Some(_) => return Err(bad),
    };
    let limit = match o.get("limit") {
        None | Some(Value::Null) => 0,
        Some(v) => usize::try_from(v.as_u64().ok_or(bad)?).unwrap_or(usize::MAX),
    };
    let query = match o.get("query") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => return Err(bad),
    };
    let ids = match o.get("user_ids") {
        None | Some(Value::Null) => None,
        // Discord also takes a single id.
        Some(v @ (Value::String(_) | Value::Number(_))) => Some(vec![v.clone()]),
        Some(Value::Array(a)) => Some(a.clone()),
        Some(_) => return Err(bad),
    };
    let (request, bad_ids) = match (query, ids) {
        (Some(_), Some(_)) | (None, None) => return Err(bad),
        (Some(query), None) => {
            let limit = if limit == 0 && !query.is_empty() {
                DEFAULT_QUERY_LIMIT
            } else {
                limit
            };
            (MemberRequest::Search { query, limit }, Vec::new())
        }
        (None, Some(raw)) => {
            if raw.len() > MAX_USER_IDS {
                return Err(bad);
            }
            let mut ids: Vec<UserId> = Vec::new();
            let mut bad_ids = Vec::new();
            for v in &raw {
                match snowflake(v) {
                    Some(id) if !ids.contains(&UserId::new(id)) => ids.push(UserId::new(id)),
                    Some(_) => {}
                    None => match v {
                        Value::String(s) => bad_ids.push(s.clone()),
                        _ => return Err(bad),
                    },
                }
            }
            (MemberRequest::Ids(ids), bad_ids)
        }
    };
    Ok(Parsed {
        guild,
        request,
        bad_ids,
        presences,
        nonce,
    })
}

impl Gateway {
    /// Serve op 8 for `session`. `Err(code)` asks the connection to close with that code.
    pub(crate) async fn request_guild_members(
        &self,
        session: &Arc<Session>,
        d: &Value,
    ) -> Result<(), u16> {
        let p = parse(d)?;
        // The whole list is for bots with the privileged intent; users search by name or id.
        let whole = matches!(&p.request, MemberRequest::Search { query, .. } if query.is_empty());
        if whole && (!session.bot || session.intents & intents::GUILD_MEMBERS == 0) {
            return Err(close::DISALLOWED_INTENTS);
        }
        if p.presences && session.intents & intents::GUILD_PRESENCES == 0 {
            return Err(close::DISALLOWED_INTENTS);
        }
        if !session.guilds().contains(&p.guild) || !session.owns_guild(p.guild) {
            return Ok(());
        }
        let Some(found) = self.inner.dir.request_members(p.guild, &p.request).await else {
            return Ok(());
        };
        let mut not_found: Vec<String> = found.not_found.iter().map(ToString::to_string).collect();
        not_found.extend(p.bad_ids);
        let chunks: Vec<&[Value]> = if found.members.is_empty() {
            vec![&[]]
        } else {
            found.members.chunks(CHUNK_SIZE).collect()
        };
        let count = chunks.len();
        for (i, members) in chunks.into_iter().enumerate() {
            let mut body = json!({
                "guild_id": p.guild,
                "members": members,
                "chunk_index": i,
                "chunk_count": count,
            });
            if i == 0 && !not_found.is_empty() {
                body["not_found"] = json!(not_found);
            }
            if p.presences {
                body["presences"] = Value::Array(self.presences_of(members));
            }
            if let Some(n) = &p.nonce {
                body["nonce"] = json!(n);
            }
            session.deliver("GUILD_MEMBERS_CHUNK", body);
        }
        Ok(())
    }

    /// Presences of the online members among `members` (member objects with `user.id`).
    fn presences_of(&self, members: &[Value]) -> Vec<Value> {
        let st = self.st();
        members
            .iter()
            .filter_map(|m| {
                let id = m.pointer("/user/id").and_then(snowflake).map(UserId::new)?;
                let p = Self::effective(&st, id)?;
                (p.visible_status() != "offline").then(|| friend_presence_body(id, &p))
            })
            .collect()
    }
}

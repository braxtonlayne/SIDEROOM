//! The lazy member list registry (LNG-GLD-010): who subscribed to which ranges of which
//! channel's list, and the refresh that keeps their slices current. The list itself is built
//! and diffed in [`crate::member_list`].
//!
//! One list exists per `(guild, channel)` while somebody is subscribed. Its members come from
//! the [`GuildDirectory`](crate::traits::GuildDirectory); presence is read live from the hub.
//! Membership, role, channel and profile events refetch the members; presence changes only
//! rebuild from what is cached. Bursts are coalesced into one refresh per guild.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use sr_core::{ChannelId, GuildId, UserId};
use sr_events::DomainEvent;

use crate::hub::Gateway;
use crate::member_list::{
    Built, MemberListSource, build, diff_range, dispatch_body, parse_ranges, sync,
};
use crate::protocol::PresenceUpdate;
use crate::session::Session;

/// Wait this long after a trigger before refreshing, so a burst costs one rebuild.
pub(crate) const REFRESH_DEBOUNCE: Duration = Duration::from_millis(75);
/// Most channels one op 102 may name.
const MAX_CHANNELS: usize = 5;

/// One live list.
pub(crate) struct ListState {
    id: String,
    base: MemberListSource,
    built: Built,
    /// Session id -> the ranges it holds.
    subs: HashMap<String, Vec<(usize, usize)>>,
}

impl ListState {
    fn list_id(source: &MemberListSource, channel: ChannelId) -> String {
        if source.everyone {
            "everyone".to_owned()
        } else {
            channel.to_string()
        }
    }
}

/// Events after which the members of a list may differ.
pub(crate) fn changes_members(ev: &DomainEvent) -> bool {
    matches!(
        ev,
        DomainEvent::MemberJoined { .. }
            | DomainEvent::MemberLeft { .. }
            | DomainEvent::MemberUpdated { .. }
            | DomainEvent::MemberBanned { .. }
            | DomainEvent::RoleCreated { .. }
            | DomainEvent::RoleUpdated { .. }
            | DomainEvent::RoleDeleted { .. }
            | DomainEvent::ChannelUpdated { .. }
            | DomainEvent::ChannelDeleted { .. }
            | DomainEvent::GuildUpdated { .. }
            | DomainEvent::UserUpdated { .. }
    )
}

impl Gateway {
    /// Presence as other users see it, for every user with a session.
    fn live_presence(&self) -> HashMap<UserId, PresenceUpdate> {
        let st = self.st();
        let users: HashSet<UserId> = st.sessions.values().map(|s| s.user).collect();
        users
            .into_iter()
            .filter_map(|u| Self::effective(&st, u).map(|p| (u, p)))
            .collect()
    }

    fn build_list(&self, base: &MemberListSource) -> Built {
        let live = self.live_presence();
        build(&base.members, &|u| {
            live.get(&u)
                .cloned()
                .unwrap_or_else(PresenceUpdate::offline)
        })
    }

    /// Op 102: the session names the ranges it wants of some channels' lists. Every channel
    /// the session cannot see is skipped; a channel with no valid range is unsubscribed. Each
    /// accepted channel is answered with one `GUILD_MEMBER_LIST_UPDATE` carrying a `SYNC` per
    /// range.
    pub(crate) async fn lazy_request(&self, session: &Arc<Session>, d: &Value) {
        let Some(guild) = d
            .get("guild_id")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<u64>().ok())
            .map(GuildId::new)
        else {
            return;
        };
        if !session.guilds().contains(&guild) {
            return;
        }
        let Some(channels) = d.get("channels").and_then(Value::as_object) else {
            return;
        };
        for (cid, raw) in channels.iter().take(MAX_CHANNELS) {
            let Some(channel) = cid.parse::<u64>().ok().map(ChannelId::new) else {
                continue;
            };
            let ranges = parse_ranges(raw);
            if ranges.is_empty() {
                self.unsubscribe_list(&session.id, (guild, channel));
                continue;
            }
            if !self
                .inner
                .dir
                .can_view_channel(session.user, guild, channel)
                .await
            {
                continue;
            }
            if !self.ensure_list(guild, channel).await {
                continue;
            }
            let body = {
                let mut st = self.st();
                let Some(list) = st.lists.get_mut(&(guild, channel)) else {
                    continue;
                };
                list.subs.insert(session.id.clone(), ranges.clone());
                let ops: Vec<_> = ranges.iter().map(|r| sync(&list.built.items, *r)).collect();
                dispatch_body(&guild.to_string(), &list.id, &list.built, &ops)
            };
            session.deliver("GUILD_MEMBER_LIST_UPDATE", body);
        }
    }

    /// Make sure the list of `(guild, channel)` exists, fetching its members when it does not.
    async fn ensure_list(&self, guild: GuildId, channel: ChannelId) -> bool {
        if self.st().lists.contains_key(&(guild, channel)) {
            return true;
        }
        let Some(base) = self.inner.dir.member_list(guild, channel).await else {
            return false;
        };
        let built = self.build_list(&base);
        let mut st = self.st();
        st.lists
            .entry((guild, channel))
            .or_insert_with(|| ListState {
                id: ListState::list_id(&base, channel),
                base,
                built,
                subs: HashMap::new(),
            });
        true
    }

    fn unsubscribe_list(&self, session_id: &str, key: (GuildId, ChannelId)) {
        let mut st = self.st();
        if let Some(list) = st.lists.get_mut(&key) {
            list.subs.remove(session_id);
            if list.subs.is_empty() {
                st.lists.remove(&key);
            }
        }
    }

    /// Forget everything `session_id` subscribed to (the session ended, or left the guild).
    pub(crate) fn drop_list_subs(
        st: &mut crate::hub::State,
        session_id: &str,
        only_guild: Option<GuildId>,
    ) {
        st.lists.retain(|(g, _), list| {
            if only_guild.is_none_or(|og| og == *g) {
                list.subs.remove(session_id);
            }
            !list.subs.is_empty()
        });
    }

    /// Something changed in `guild`: refresh its lists soon. `refetch` also reloads the
    /// members (membership, roles, profiles); without it only presence is re-read.
    pub(crate) fn schedule_list_refresh(&self, guild: GuildId, refetch: bool) {
        let spawn = {
            let mut st = self.st();
            if !st.lists.keys().any(|k| k.0 == guild) {
                return;
            }
            match st.list_dirty.get_mut(&guild) {
                Some(r) => {
                    *r |= refetch;
                    false
                }
                None => {
                    st.list_dirty.insert(guild, refetch);
                    true
                }
            }
        };
        if !spawn {
            return;
        }
        let weak = Arc::downgrade(&self.inner);
        tokio::spawn(async move {
            tokio::time::sleep(REFRESH_DEBOUNCE).await;
            let Some(inner) = weak.upgrade() else { return };
            let gw = Gateway { inner };
            let refetch = gw.st().list_dirty.remove(&guild).unwrap_or(false);
            gw.refresh_member_lists(guild, refetch).await;
        });
    }

    /// Rebuild every list of `guild` and send each subscriber the ops that bring its ranges up
    /// to date.
    pub(crate) async fn refresh_member_lists(&self, guild: GuildId, refetch: bool) {
        let keys: Vec<(GuildId, ChannelId)> = self
            .st()
            .lists
            .keys()
            .filter(|k| k.0 == guild)
            .copied()
            .collect();
        for key in keys {
            let fresh = if refetch {
                self.inner.dir.member_list(key.0, key.1).await
            } else {
                None
            };
            // Subscribers who can no longer see the channel stop getting its list.
            let mut denied: HashSet<UserId> = HashSet::new();
            if refetch {
                let users: HashSet<UserId> = {
                    let st = self.st();
                    st.lists
                        .get(&key)
                        .into_iter()
                        .flat_map(|l| l.subs.keys())
                        .filter_map(|sid| st.sessions.get(sid).map(|s| s.user))
                        .collect()
                };
                for u in users {
                    if !self.inner.dir.can_view_channel(u, key.0, key.1).await {
                        denied.insert(u);
                    }
                }
            }
            let live = self.live_presence();
            let sends: Vec<(Arc<Session>, Value)> = {
                let mut st = self.st();
                let sessions = st.sessions.clone();
                let Some(list) = st.lists.get_mut(&key) else {
                    continue;
                };
                if let Some(base) = fresh {
                    list.id = ListState::list_id(&base, key.1);
                    list.base = base;
                }
                let built = build(&list.base.members, &|u| {
                    live.get(&u)
                        .cloned()
                        .unwrap_or_else(PresenceUpdate::offline)
                });
                let mut sends = Vec::new();
                list.subs.retain(|sid, ranges| {
                    let Some(s) = sessions.get(sid) else {
                        return false;
                    };
                    if denied.contains(&s.user) {
                        return false;
                    }
                    let ops: Vec<_> = ranges
                        .iter()
                        .flat_map(|r| diff_range(&list.built.items, &built.items, *r))
                        .collect();
                    if !ops.is_empty() {
                        sends.push((
                            s.clone(),
                            dispatch_body(&key.0.to_string(), &list.id, &built, &ops),
                        ));
                    }
                    true
                });
                list.built = built;
                if list.subs.is_empty() {
                    st.lists.remove(&key);
                }
                sends
            };
            for (s, body) in sends {
                s.deliver("GUILD_MEMBER_LIST_UPDATE", body);
            }
        }
    }
}

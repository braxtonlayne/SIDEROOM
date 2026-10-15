//! In-memory implementation of the social repository traits, for tests of this
//! crate and of consumers that need a database-free fake.

use super::dm::{DmChannel, DmKind, GroupInvite, RequestState};
use super::notifications::{ChannelOverride, NotificationLevel, ScopeSettings};
use super::presence::PresencePrefs;
use super::profile::{Connection, Profile};
use super::read_state::{MAX_MENTION_COUNT, ReadState};
use super::relationships::{RelOp, Relationship, RelationshipCounts, RelationshipKind};
use super::repo::*;
use super::user_settings::UserSettings;
use crate::error::RepoError;
use async_trait::async_trait;
use sr_core::{ChannelId, GuildId, UserId};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

#[derive(Default)]
pub struct Store {
    pub users: HashSet<UserId>,
    pub rels: HashMap<(UserId, UserId), Relationship>,
    pub privacy: HashMap<UserId, Privacy>,
    /// Pairs (ordered) that share a guild; set by tests.
    pub shared_guilds: HashSet<(UserId, UserId)>,
    /// Guild memberships `(guild, user) -> (nick, joined_at_ms)`; set by tests.
    pub members: HashMap<(GuildId, UserId), (Option<String>, i64)>,
    /// Lower-cased username to user; set by tests.
    pub usernames: HashMap<String, UserId>,
    pub dms: HashMap<ChannelId, DmChannel>,
    pub hidden: HashSet<(UserId, ChannelId)>,
    pub read_states: HashMap<(UserId, ChannelId), ReadState>,
    pub scopes: HashMap<(UserId, Option<GuildId>), ScopeSettings>,
    pub overrides: HashMap<(UserId, ChannelId), ChannelOverride>,
    pub profiles: HashMap<UserId, Profile>,
    pub connections: Vec<Connection>,
    pub presence: HashMap<UserId, PresencePrefs>,
    /// Ordered `(ignorer, ignored)` pairs.
    pub ignores: HashSet<(UserId, UserId)>,
    pub notes: HashMap<(UserId, UserId), String>,
    /// Message Request state per `(recipient, channel)`; absent means accepted.
    pub requests: HashMap<(UserId, ChannelId), RequestState>,
    /// Group DM invites by code (G-47).
    pub group_invites: HashMap<String, GroupInvite>,
    pub user_settings: HashMap<UserId, super::user_settings::UserSettings>,
    pub default_levels: HashMap<UserId, super::notifications::NotificationLevel>,
}

#[derive(Default)]
pub struct MemoryRepo {
    pub store: Mutex<Store>,
}

fn pair(a: UserId, b: UserId) -> (UserId, UserId) {
    if a < b { (a, b) } else { (b, a) }
}

impl MemoryRepo {
    fn lock(&self) -> RepoResult<std::sync::MutexGuard<'_, Store>> {
        self.store
            .lock()
            .map_err(|_| RepoError("store poisoned".into()))
    }

    /// Test helper: register a user so `user_exists` is true.
    pub fn add_user(&self, user: UserId) {
        if let Ok(mut s) = self.lock() {
            s.users.insert(user);
        }
    }

    /// Test helper: register a user with a username (looked up case-insensitively).
    pub fn add_named_user(&self, user: UserId, username: &str) {
        if let Ok(mut s) = self.lock() {
            s.users.insert(user);
            s.usernames.insert(username.to_lowercase(), user);
        }
    }

    /// Test helper: record a guild membership.
    pub fn add_member(&self, guild: GuildId, user: UserId, nick: Option<&str>, joined_at_ms: i64) {
        if let Ok(mut s) = self.lock() {
            s.members
                .insert((guild, user), (nick.map(str::to_owned), joined_at_ms));
        }
    }

    /// Test helper: make two users share a guild.
    pub fn share_guild(&self, a: UserId, b: UserId) {
        if let Ok(mut s) = self.lock() {
            s.shared_guilds.insert(pair(a, b));
        }
    }
}

#[async_trait]
impl RelationshipRepository for MemoryRepo {
    async fn user_exists(&self, user: UserId) -> RepoResult<bool> {
        Ok(self.lock()?.users.contains(&user))
    }

    async fn get_relationship(
        &self,
        user: UserId,
        target: UserId,
    ) -> RepoResult<Option<Relationship>> {
        Ok(self.lock()?.rels.get(&(user, target)).cloned())
    }

    async fn list_relationships(&self, user: UserId) -> RepoResult<Vec<Relationship>> {
        let s = self.lock()?;
        let mut v: Vec<_> = s
            .rels
            .values()
            .filter(|r| r.user == user)
            .cloned()
            .collect();
        v.sort_by_key(|r| (r.since_ms, r.target));
        Ok(v)
    }

    async fn counts(&self, user: UserId) -> RepoResult<RelationshipCounts> {
        let s = self.lock()?;
        let mut c = RelationshipCounts::default();
        for r in s.rels.values().filter(|r| r.user == user) {
            match r.kind {
                RelationshipKind::Friend => c.friends += 1,
                RelationshipKind::PendingOutgoing => c.pending_outgoing += 1,
                RelationshipKind::Blocked => c.blocked += 1,
                _ => {}
            }
        }
        Ok(c)
    }

    async fn apply(&self, ops: &[RelOp]) -> RepoResult<()> {
        let mut s = self.lock()?;
        for op in ops {
            match op {
                RelOp::Upsert {
                    user,
                    target,
                    kind,
                    since_ms,
                } => {
                    s.rels.insert(
                        (*user, *target),
                        Relationship {
                            user: *user,
                            target: *target,
                            kind: *kind,
                            nickname: None,
                            since_ms: *since_ms,
                        },
                    );
                }
                RelOp::Delete { user, target } => {
                    s.rels.remove(&(*user, *target));
                }
                RelOp::SetNickname {
                    user,
                    target,
                    nickname,
                } => {
                    if let Some(r) = s.rels.get_mut(&(*user, *target)) {
                        r.nickname = nickname.clone();
                    }
                }
            }
        }
        Ok(())
    }

    async fn shares_guild(&self, a: UserId, b: UserId) -> RepoResult<bool> {
        let s = self.lock()?;
        Ok(s.shared_guilds.contains(&pair(a, b))
            || s.members
                .keys()
                .any(|(g, u)| *u == a && s.members.contains_key(&(*g, b))))
    }

    async fn mutual_friends(&self, a: UserId, b: UserId) -> RepoResult<Vec<UserId>> {
        let s = self.lock()?;
        let friends = |u: UserId| -> HashSet<UserId> {
            s.rels
                .values()
                .filter(|r| r.user == u && r.kind == RelationshipKind::Friend)
                .map(|r| r.target)
                .collect()
        };
        let mut out: Vec<UserId> = friends(a).intersection(&friends(b)).copied().collect();
        out.sort();
        Ok(out)
    }

    async fn find_user_by_username(&self, username: &str) -> RepoResult<Option<UserId>> {
        Ok(self
            .lock()?
            .usernames
            .get(&username.to_lowercase())
            .copied())
    }

    async fn list_guild_memberships(&self, user: UserId) -> RepoResult<Vec<GuildMembership>> {
        let s = self.lock()?;
        let mut out: Vec<GuildMembership> = s
            .members
            .iter()
            .filter(|((_, u), _)| *u == user)
            .map(|((g, _), (nick, at))| GuildMembership {
                guild: *g,
                nick: nick.clone(),
                joined_at_ms: *at,
                ..GuildMembership::default()
            })
            .collect();
        out.sort_by_key(|m| m.guild);
        Ok(out)
    }

    async fn guild_peers(&self, user: UserId) -> RepoResult<Vec<UserId>> {
        let s = self.lock()?;
        let mine: HashSet<GuildId> = s
            .members
            .keys()
            .filter(|(_, u)| *u == user)
            .map(|(g, _)| *g)
            .collect();
        let mut out: HashSet<UserId> = s
            .members
            .keys()
            .filter(|(g, u)| *u != user && mine.contains(g))
            .map(|(_, u)| *u)
            .collect();
        for (a, b) in &s.shared_guilds {
            if *a == user {
                out.insert(*b);
            } else if *b == user {
                out.insert(*a);
            }
        }
        let mut out: Vec<UserId> = out.into_iter().collect();
        out.sort();
        Ok(out)
    }

    async fn has_mutual_friend(&self, a: UserId, b: UserId) -> RepoResult<bool> {
        let s = self.lock()?;
        let friends = |u: UserId| -> HashSet<UserId> {
            s.rels
                .values()
                .filter(|r| r.user == u && r.kind == RelationshipKind::Friend)
                .map(|r| r.target)
                .collect()
        };
        Ok(friends(a).intersection(&friends(b)).next().is_some())
    }

    async fn get_privacy(&self, user: UserId) -> RepoResult<Privacy> {
        Ok(self.lock()?.privacy.get(&user).copied().unwrap_or_default())
    }

    async fn put_privacy(&self, user: UserId, privacy: &Privacy) -> RepoResult<()> {
        self.lock()?.privacy.insert(user, *privacy);
        Ok(())
    }

    async fn is_ignoring(&self, user: UserId, target: UserId) -> RepoResult<bool> {
        Ok(self.lock()?.ignores.contains(&(user, target)))
    }

    async fn list_ignores(&self, user: UserId) -> RepoResult<Vec<UserId>> {
        let mut out: Vec<UserId> = self
            .lock()?
            .ignores
            .iter()
            .filter(|(u, _)| *u == user)
            .map(|(_, t)| *t)
            .collect();
        out.sort();
        Ok(out)
    }

    async fn put_ignore(&self, user: UserId, target: UserId) -> RepoResult<()> {
        self.lock()?.ignores.insert((user, target));
        Ok(())
    }

    async fn delete_ignore(&self, user: UserId, target: UserId) -> RepoResult<()> {
        self.lock()?.ignores.remove(&(user, target));
        Ok(())
    }

    async fn get_note(&self, user: UserId, target: UserId) -> RepoResult<Option<String>> {
        Ok(self.lock()?.notes.get(&(user, target)).cloned())
    }

    async fn set_note(&self, user: UserId, target: UserId, note: Option<&str>) -> RepoResult<()> {
        let mut s = self.lock()?;
        match note {
            Some(n) => {
                s.notes.insert((user, target), n.to_owned());
            }
            None => {
                s.notes.remove(&(user, target));
            }
        }
        Ok(())
    }
}

#[async_trait]
impl DmRepository for MemoryRepo {
    async fn find_dm(&self, a: UserId, b: UserId) -> RepoResult<Option<DmChannel>> {
        let s = self.lock()?;
        Ok(s.dms
            .values()
            .find(|d| d.kind == DmKind::Dm && d.has(a) && d.has(b))
            .cloned())
    }

    async fn insert_dm(&self, id: ChannelId, a: UserId, b: UserId) -> RepoResult<()> {
        let (x, y) = pair(a, b);
        let mut s = self.lock()?;
        if s.dms
            .values()
            .any(|d| d.kind == DmKind::Dm && d.has(x) && d.has(y))
        {
            return Err(RepoError("duplicate dm pair".into()));
        }
        s.dms.insert(
            id,
            DmChannel {
                id,
                kind: DmKind::Dm,
                owner: None,
                name: None,
                icon_key: None,
                recipients: vec![x, y],
                last_message_id: None,
            },
        );
        Ok(())
    }

    async fn insert_group(
        &self,
        id: ChannelId,
        members: &[UserId],
        name: Option<&str>,
    ) -> RepoResult<()> {
        self.lock()?.dms.insert(
            id,
            DmChannel {
                id,
                kind: DmKind::Group,
                owner: members.first().copied(),
                name: name.map(str::to_owned),
                icon_key: None,
                recipients: members.to_vec(),
                last_message_id: None,
            },
        );
        Ok(())
    }

    async fn get_dm(&self, id: ChannelId) -> RepoResult<Option<DmChannel>> {
        Ok(self.lock()?.dms.get(&id).cloned())
    }

    async fn add_recipients(&self, id: ChannelId, users: &[UserId]) -> RepoResult<()> {
        let mut s = self.lock()?;
        let d = s.dms.get_mut(&id).ok_or(RepoError("no such dm".into()))?;
        for u in users {
            if !d.recipients.contains(u) {
                d.recipients.push(*u);
            }
        }
        Ok(())
    }

    async fn remove_recipient(&self, id: ChannelId, user: UserId) -> RepoResult<()> {
        let mut s = self.lock()?;
        if let Some(d) = s.dms.get_mut(&id) {
            d.recipients.retain(|u| *u != user);
        }
        s.hidden.remove(&(user, id));
        Ok(())
    }

    async fn set_owner(&self, id: ChannelId, owner: UserId) -> RepoResult<()> {
        if let Some(d) = self.lock()?.dms.get_mut(&id) {
            d.owner = Some(owner);
        }
        Ok(())
    }

    async fn update_group(
        &self,
        id: ChannelId,
        name: Option<&str>,
        icon_key: Option<&str>,
    ) -> RepoResult<()> {
        if let Some(d) = self.lock()?.dms.get_mut(&id) {
            d.name = name.map(str::to_owned);
            d.icon_key = icon_key.map(str::to_owned);
        }
        Ok(())
    }

    async fn delete_dm(&self, id: ChannelId) -> RepoResult<()> {
        let mut s = self.lock()?;
        s.dms.remove(&id);
        s.hidden.retain(|(_, c)| *c != id);
        Ok(())
    }

    async fn set_hidden(&self, user: UserId, id: ChannelId, hidden: bool) -> RepoResult<bool> {
        let mut s = self.lock()?;
        let key = (user, id);
        Ok(if hidden {
            s.hidden.insert(key)
        } else {
            s.hidden.remove(&key)
        })
    }

    async fn list_dms(&self, user: UserId, include_hidden: bool) -> RepoResult<Vec<DmChannel>> {
        let s = self.lock()?;
        let mut v: Vec<_> = s
            .dms
            .values()
            .filter(|d| d.has(user) && (include_hidden || !s.hidden.contains(&(user, d.id))))
            .cloned()
            .collect();
        v.sort_by_key(|d| std::cmp::Reverse((d.last_message_id, d.id)));
        Ok(v)
    }

    async fn count_groups(&self, user: UserId) -> RepoResult<usize> {
        let s = self.lock()?;
        Ok(s.dms
            .values()
            .filter(|d| d.kind == DmKind::Group && d.has(user))
            .count())
    }

    async fn get_request_state(
        &self,
        user: UserId,
        id: ChannelId,
    ) -> RepoResult<Option<RequestState>> {
        let s = self.lock()?;
        let Some(d) = s.dms.get(&id) else {
            return Ok(None);
        };
        if !d.has(user) {
            return Ok(None);
        }
        Ok(Some(
            s.requests
                .get(&(user, id))
                .copied()
                .unwrap_or(RequestState::Accepted),
        ))
    }

    async fn set_request_state(
        &self,
        user: UserId,
        id: ChannelId,
        state: RequestState,
    ) -> RepoResult<()> {
        self.lock()?.requests.insert((user, id), state);
        Ok(())
    }

    async fn insert_group_invite(&self, invite: &GroupInvite) -> RepoResult<()> {
        self.lock()?
            .group_invites
            .insert(invite.code.clone(), invite.clone());
        Ok(())
    }

    async fn get_group_invite(&self, code: &str) -> RepoResult<Option<GroupInvite>> {
        Ok(self.lock()?.group_invites.get(code).cloned())
    }

    async fn revoke_group_invite(&self, code: &str) -> RepoResult<bool> {
        Ok(match self.lock()?.group_invites.get_mut(code) {
            Some(i) => {
                i.revoked = true;
                true
            }
            None => false,
        })
    }

    async fn list_group_invites(&self, channel: ChannelId) -> RepoResult<Vec<GroupInvite>> {
        let mut v: Vec<_> = self
            .lock()?
            .group_invites
            .values()
            .filter(|i| i.channel_id == channel)
            .cloned()
            .collect();
        v.sort_by_key(|i| std::cmp::Reverse(i.created_at_ms));
        Ok(v)
    }
}

#[async_trait]
impl ReadStateRepository for MemoryRepo {
    async fn get_read_state(
        &self,
        user: UserId,
        channel: ChannelId,
    ) -> RepoResult<Option<ReadState>> {
        Ok(self.lock()?.read_states.get(&(user, channel)).cloned())
    }

    async fn put_read_state(&self, state: &ReadState) -> RepoResult<()> {
        self.lock()?
            .read_states
            .insert((state.user, state.channel), state.clone());
        Ok(())
    }

    async fn list_read_states(&self, user: UserId) -> RepoResult<Vec<ReadState>> {
        let s = self.lock()?;
        let mut v: Vec<_> = s
            .read_states
            .values()
            .filter(|r| r.user == user)
            .cloned()
            .collect();
        v.sort_by_key(|r| r.channel);
        Ok(v)
    }

    async fn add_mentions(&self, users: &[UserId], channel: ChannelId, n: u32) -> RepoResult<()> {
        let mut s = self.lock()?;
        for u in users {
            let e = s
                .read_states
                .entry((*u, channel))
                .or_insert_with(|| ReadState::new(*u, channel));
            e.mention_count = e.mention_count.saturating_add(n).min(MAX_MENTION_COUNT);
        }
        Ok(())
    }
}

#[async_trait]
impl NotificationRepository for MemoryRepo {
    async fn get_scope(
        &self,
        user: UserId,
        guild: Option<GuildId>,
    ) -> RepoResult<Option<ScopeSettings>> {
        Ok(self.lock()?.scopes.get(&(user, guild)).cloned())
    }

    async fn put_scope(&self, settings: &ScopeSettings) -> RepoResult<()> {
        self.lock()?
            .scopes
            .insert((settings.user, settings.guild), settings.clone());
        Ok(())
    }

    async fn get_override(
        &self,
        user: UserId,
        channel: ChannelId,
    ) -> RepoResult<Option<ChannelOverride>> {
        Ok(self.lock()?.overrides.get(&(user, channel)).cloned())
    }

    async fn put_override(&self, o: &ChannelOverride) -> RepoResult<()> {
        self.lock()?
            .overrides
            .insert((o.user, o.channel), o.clone());
        Ok(())
    }

    async fn delete_override(&self, user: UserId, channel: ChannelId) -> RepoResult<()> {
        self.lock()?.overrides.remove(&(user, channel));
        Ok(())
    }

    async fn list_overrides(&self, user: UserId) -> RepoResult<Vec<ChannelOverride>> {
        let s = self.lock()?;
        let mut v: Vec<_> = s
            .overrides
            .values()
            .filter(|o| o.user == user)
            .cloned()
            .collect();
        v.sort_by_key(|o| o.channel);
        Ok(v)
    }
}

#[async_trait]
impl ProfileRepository for MemoryRepo {
    async fn get_profile(&self, user: UserId) -> RepoResult<Option<Profile>> {
        let s = self.lock()?;
        let Some(mut p) = s.profiles.get(&user).cloned() else {
            return Ok(None);
        };
        p.connections = s
            .connections
            .iter()
            .filter(|c| c.user == user)
            .cloned()
            .collect();
        Ok(Some(p))
    }

    async fn put_profile(&self, profile: &Profile) -> RepoResult<()> {
        let mut p = profile.clone();
        p.connections.clear();
        self.lock()?.profiles.insert(profile.user, p);
        Ok(())
    }

    async fn upsert_connection(&self, c: &Connection) -> RepoResult<()> {
        let mut s = self.lock()?;
        // A re-link keeps an existing verified flag (only OAuth proves a link).
        let was_verified = s.connections.iter().any(|x| {
            x.user == c.user
                && x.provider == c.provider
                && x.external_id == c.external_id
                && x.verified
        });
        s.connections.retain(|x| {
            !(x.user == c.user && x.provider == c.provider && x.external_id == c.external_id)
        });
        let mut stored = c.clone();
        stored.verified |= was_verified;
        s.connections.push(stored);
        Ok(())
    }

    async fn delete_connection(
        &self,
        user: UserId,
        provider: &str,
        external_id: &str,
    ) -> RepoResult<()> {
        self.lock()?.connections.retain(|x| {
            !(x.user == user && x.provider == provider && x.external_id == external_id)
        });
        Ok(())
    }

    async fn count_connections(&self, user: UserId) -> RepoResult<usize> {
        Ok(self
            .lock()?
            .connections
            .iter()
            .filter(|c| c.user == user)
            .count())
    }

    async fn connection_owner(
        &self,
        provider: &str,
        external_id: &str,
    ) -> RepoResult<Option<UserId>> {
        Ok(self
            .lock()?
            .connections
            .iter()
            .find(|c| c.verified && c.provider == provider && c.external_id == external_id)
            .map(|c| c.user))
    }

    async fn stale_connections(
        &self,
        _before_ms: i64,
        providers: &[&str],
        limit: usize,
    ) -> RepoResult<Vec<Connection>> {
        Ok(self
            .lock()?
            .connections
            .iter()
            .filter(|c| c.verified && c.show_activity && providers.contains(&c.provider.as_str()))
            .take(limit)
            .cloned()
            .collect())
    }

    async fn set_connection_stats(
        &self,
        user: UserId,
        provider: &str,
        external_id: &str,
        stats: &serde_json::Value,
        _now_ms: i64,
    ) -> RepoResult<()> {
        for c in self.lock()?.connections.iter_mut() {
            if c.user == user && c.provider == provider && c.external_id == external_id {
                c.stats = stats.clone();
            }
        }
        Ok(())
    }
}

#[async_trait]
impl PresenceRepository for MemoryRepo {
    async fn get_presence(&self, user: UserId) -> RepoResult<Option<PresencePrefs>> {
        Ok(self.lock()?.presence.get(&user).cloned())
    }

    async fn put_presence(&self, prefs: &PresencePrefs) -> RepoResult<()> {
        self.lock()?.presence.insert(prefs.user, prefs.clone());
        Ok(())
    }
}

#[async_trait]
impl UserSettingsRepository for MemoryRepo {
    async fn get_user_settings(&self, user: UserId) -> RepoResult<Option<UserSettings>> {
        Ok(self.lock()?.user_settings.get(&user).cloned())
    }

    async fn put_user_settings(
        &self,
        user: UserId,
        data: &serde_json::Value,
        expected_version: i64,
    ) -> RepoResult<Option<i64>> {
        let mut s = self.lock()?;
        let current = s.user_settings.get(&user).map_or(0, |u| u.version);
        if current != expected_version {
            return Ok(None);
        }
        let version = current + 1;
        s.user_settings.insert(
            user,
            UserSettings {
                data: data.clone(),
                version,
            },
        );
        Ok(Some(version))
    }

    async fn get_default_notification_level(
        &self,
        user: UserId,
    ) -> RepoResult<Option<NotificationLevel>> {
        Ok(self.lock()?.default_levels.get(&user).copied())
    }

    async fn put_default_notification_level(
        &self,
        user: UserId,
        level: Option<NotificationLevel>,
    ) -> RepoResult<()> {
        let mut s = self.lock()?;
        match level {
            Some(l) => s.default_levels.insert(user, l),
            None => s.default_levels.remove(&user),
        };
        Ok(())
    }
}

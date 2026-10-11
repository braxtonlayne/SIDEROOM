//! Collaborators of the search endpoints: the message index, the viewable-channel
//! cache and the age gate. Registered on the composition root as an
//! [`AppState`](crate::AppState) extension and read with [`crate::routes::Ext`].

use async_trait::async_trait;
use moka::sync::Cache;
use sr_chat::ChannelType;
use sr_chat::perms::channel_permissions;
use sr_chat::repo::ChatRepository;
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, GuildId, UserId};
use sr_events::DomainEvent;
use sr_search::{ACCESS_TTL, MessageIndex, ViewableChannelCache};
use std::collections::HashSet;
use std::sync::Arc;

use crate::HttpError;
use crate::routes::settings::deps::re;
use sr_core::ErrorCode;

/// Whether an account confirmed it is 18 or older (`accounts.age_confirmed_at`).
/// NSFW channels are left out of search results otherwise (open question 131).
#[async_trait]
pub trait AgeGate: Send + Sync {
    async fn age_confirmed(&self, user: UserId) -> bool;
}

/// Nobody has confirmed their age; NSFW channels stay hidden.
pub struct NoAgeConfirmed;

#[async_trait]
impl AgeGate for NoAgeConfirmed {
    async fn age_confirmed(&self, _: UserId) -> bool {
        false
    }
}

/// A fixed set of confirmed users (tests, tools).
pub struct ConfirmedUsers(pub HashSet<UserId>);

#[async_trait]
impl AgeGate for ConfirmedUsers {
    async fn age_confirmed(&self, user: UserId) -> bool {
        self.0.contains(&user)
    }
}

pub struct SearchDeps {
    pub index: Arc<MessageIndex>,
    /// Per-(user, guild) channels the user may read, 60 s TTL.
    pub viewable: ViewableChannelCache,
    nsfw: Cache<GuildId, Arc<HashSet<ChannelId>>>,
    age: Arc<dyn AgeGate>,
}

impl SearchDeps {
    pub fn new(index: Arc<MessageIndex>) -> Self {
        Self {
            index,
            viewable: ViewableChannelCache::new(),
            nsfw: Cache::builder().time_to_live(ACCESS_TTL).build(),
            age: Arc::new(NoAgeConfirmed),
        }
    }

    pub fn with_age_gate(mut self, age: Arc<dyn AgeGate>) -> Self {
        self.age = age;
        self
    }

    pub fn with_cache(mut self, viewable: ViewableChannelCache) -> Self {
        self.viewable = viewable;
        self
    }

    pub async fn age_confirmed(&self, user: UserId) -> bool {
        self.age.age_confirmed(user).await
    }

    /// Drop cached allow-lists after a permission-relevant change (open question
    /// 131): roles, channels and overwrites invalidate the whole guild, a member
    /// change only that member.
    pub fn apply_event(&self, ev: &DomainEvent) {
        match ev {
            DomainEvent::RoleCreated { guild, .. }
            | DomainEvent::RoleUpdated { guild, .. }
            | DomainEvent::RoleDeleted { guild, .. }
            | DomainEvent::ChannelCreatedFull { guild, .. }
            | DomainEvent::ChannelUpdated { guild, .. }
            | DomainEvent::ChannelDeleted { guild, .. }
            | DomainEvent::GuildUpdated { guild, .. }
            | DomainEvent::GuildDeleted { guild } => {
                self.viewable.invalidate_guild(*guild);
                self.nsfw.invalidate(guild);
            }
            DomainEvent::MemberUpdated { guild, user, .. }
            | DomainEvent::MemberJoined { guild, user, .. }
            | DomainEvent::MemberLeft { guild, user }
            | DomainEvent::MemberBanned { guild, user, .. }
            | DomainEvent::MemberUnbanned { guild, user, .. } => {
                self.viewable.invalidate_member(*user, *guild);
            }
            _ => {}
        }
    }

    /// Channels of `guild` the user may search: `VIEW_CHANNEL` and
    /// `READ_MESSAGE_HISTORY`, NSFW channels only for age-confirmed accounts.
    /// A non-member gets 403.
    pub async fn searchable_channels(
        &self,
        repo: &dyn ChatRepository,
        user: UserId,
        guild: GuildId,
        now_ms: i64,
    ) -> Result<Vec<ChannelId>, HttpError> {
        let allowed = match self.viewable.get(user, guild) {
            Some(hit) => hit,
            None => {
                let (list, nsfw) = compute_viewable(repo, user, guild, now_ms).await?;
                self.nsfw.insert(guild, Arc::new(nsfw));
                self.viewable
                    .get_or_compute::<HttpError>(user, guild, || Ok(list))?
            }
        };
        let nsfw = match self.nsfw.get(&guild) {
            Some(n) => n,
            None => {
                // The allow-list outlived the NSFW set; rebuild just the set.
                let n = Arc::new(nsfw_channels(repo, guild).await?);
                self.nsfw.insert(guild, n.clone());
                n
            }
        };
        let confirmed = self.age_confirmed(user).await;
        Ok(sr_search::apply_nsfw_gate(&allowed, &nsfw, confirmed))
    }
}

async fn nsfw_channels(
    repo: &dyn ChatRepository,
    guild: GuildId,
) -> Result<HashSet<ChannelId>, HttpError> {
    Ok(repo
        .list_guild_channels(guild)
        .await
        .map_err(re)?
        .into_iter()
        .filter(|c| c.nsfw)
        .map(|c| c.id)
        .collect())
}

async fn compute_viewable(
    repo: &dyn ChatRepository,
    user: UserId,
    guild: GuildId,
    now_ms: i64,
) -> Result<(Vec<ChannelId>, HashSet<ChannelId>), HttpError> {
    let g = repo
        .get_guild(guild)
        .await
        .map_err(re)?
        .ok_or(ErrorCode::UnknownGuild)?;
    let member = repo
        .get_member(guild, user)
        .await
        .map_err(re)?
        .ok_or(ErrorCode::MissingAccess)?;
    let roles = repo.list_roles(guild).await.map_err(re)?;
    let mut out = Vec::new();
    let mut nsfw = HashSet::new();
    for ch in repo.list_guild_channels(guild).await.map_err(re)? {
        if ch.nsfw {
            nsfw.insert(ch.id);
        }
        if ch.kind == ChannelType::Category {
            continue;
        }
        let overwrites = repo.get_overwrites(ch.id).await.map_err(re)?;
        let perms = channel_permissions(&g, &member, &roles, &overwrites, now_ms);
        if perms.has(Permissions::VIEW_CHANNEL) && perms.has(Permissions::READ_MESSAGE_HISTORY) {
            out.push(ch.id);
        }
    }
    Ok((out, nsfw))
}

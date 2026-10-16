//! Wiring of Activities (DEV-ACT): the SSRF-safe fetcher behind the activity proxy and entry submission, and
//! channel permissions for launching and joining from the chat repository.

use std::sync::Arc;

use async_trait::async_trait;
use sr_activities::{ChannelAccess, ChannelPerms};
use sr_api::routes::activities::{ActivityFetcher, Fetched, Forwarded};
use sr_chat::repo::ChatRepository;
use sr_chat::threads::access::guild_channel_permissions;
use sr_chat::{ChannelType, perms};
use sr_core::permissions::Permissions;
use sr_core::{ChannelId, Clock, UserId};
use sr_net::{ForwardMethod, NetError, Policy, SafeClient};
use url::Url;

/// [`ActivityFetcher`] over [`SafeClient`]: public addresses only, every redirect hop validated again.
pub struct SafeActivityFetcher(SafeClient);

impl SafeActivityFetcher {
    /// The production client.
    pub fn production() -> Self {
        Self(SafeClient::production(
            Policy::default(),
            "SideRoom-Activities/1.0",
        ))
    }
}

/// A reason for the owner; it never contains the URL.
fn reason(e: &NetError) -> String {
    match e {
        NetError::Blocked(_) => "the address is not a public host".to_owned(),
        NetError::Timeout => "the request timed out".to_owned(),
        NetError::Resolve(_) => "the host name did not resolve".to_owned(),
        NetError::TooLarge => "the file was too large".to_owned(),
        NetError::BadUrl(_) => "the URL is not valid".to_owned(),
        NetError::BadStatus(s) => format!("the server answered {s}"),
        _ => "the connection failed".to_owned(),
    }
}

#[async_trait]
impl ActivityFetcher for SafeActivityFetcher {
    async fn get(&self, url: &str, max_bytes: usize) -> Result<Fetched, String> {
        let url = Url::parse(url).map_err(|_| "the URL is not valid".to_owned())?;
        let r = self
            .0
            .get(&url, "*/*", max_bytes)
            .await
            .map_err(|e| reason(&e))?;
        Ok(Fetched {
            content_type: r.content_type,
            body: r.body,
        })
    }

    async fn forward(
        &self,
        method: &axum::http::Method,
        url: &str,
        content_type: Option<String>,
        body: Vec<u8>,
        max_bytes: usize,
    ) -> Result<Forwarded, String> {
        let method = match *method {
            axum::http::Method::POST => ForwardMethod::Post,
            axum::http::Method::PUT => ForwardMethod::Put,
            axum::http::Method::PATCH => ForwardMethod::Patch,
            axum::http::Method::DELETE => ForwardMethod::Delete,
            _ => return Err("this method is not forwarded".to_owned()),
        };
        let url = Url::parse(url).map_err(|_| "the URL is not valid".to_owned())?;
        let r = self
            .0
            .forward(method, &url, content_type, body, max_bytes)
            .await
            .map_err(|e| reason(&e))?;
        Ok(Forwarded {
            status: r.status,
            content_type: r.content_type,
            body: r.body,
        })
    }
}

/// [`ChannelAccess`] over the chat repository.
pub struct ChatActivityChannels<R> {
    repo: R,
    clock: Arc<dyn Clock>,
}

impl<R: ChatRepository> ChatActivityChannels<R> {
    pub fn new(repo: R, clock: Arc<dyn Clock>) -> Self {
        Self { repo, clock }
    }
}

fn uid(v: i64) -> UserId {
    UserId::new(u64::try_from(v).unwrap_or(0))
}

/// At most this many members are told about an activity.
const VIEWER_CAP: usize = 5_000;

#[async_trait]
impl<R: ChatRepository + 'static> ChannelAccess for ChatActivityChannels<R> {
    async fn perms(&self, user: i64, channel: i64) -> Result<Option<ChannelPerms>, String> {
        let Some(ch) = self
            .repo
            .get_channel(ChannelId::new(u64::try_from(channel).unwrap_or(0)))
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        // Activities run in guild channels only.
        let Some(gid) = ch.guild_id else {
            return Ok(None);
        };
        let voice = matches!(ch.kind, ChannelType::Voice);
        let text = matches!(ch.kind, ChannelType::Text | ChannelType::Announcement);
        let Some(guild) = self.repo.get_guild(gid).await.map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        let Some(member) = self
            .repo
            .get_member(gid, uid(user))
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let roles = self.repo.list_roles(gid).await.map_err(|e| e.to_string())?;
        let p = guild_channel_permissions(
            &self.repo,
            &guild,
            &member,
            &roles,
            &ch,
            self.clock.now_unix_ms(),
        )
        .await
        .map_err(|e| e.to_string())?;
        if !p.has(Permissions::VIEW_CHANNEL) {
            return Ok(None);
        }
        Ok(Some(ChannelPerms {
            guild_id: i64::try_from(gid.get()).unwrap_or(0),
            voice,
            text,
            view: true,
            connect: p.has(Permissions::CONNECT),
            use_activities: p.has(Permissions::USE_EMBEDDED_ACTIVITIES),
            manage: p.has(Permissions::MANAGE_CHANNELS),
        }))
    }

    async fn viewers(&self, channel: i64) -> Result<Vec<i64>, String> {
        let Some(ch) = self
            .repo
            .get_channel(ChannelId::new(u64::try_from(channel).unwrap_or(0)))
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(Vec::new());
        };
        let Some(gid) = ch.guild_id else {
            return Ok(Vec::new());
        };
        let to_i64 = |u: UserId| i64::try_from(u.get()).unwrap_or(0);
        let Some(guild) = self.repo.get_guild(gid).await.map_err(|e| e.to_string())? else {
            return Ok(Vec::new());
        };
        let roles = self.repo.list_roles(gid).await.map_err(|e| e.to_string())?;
        let overwrites = self
            .repo
            .get_overwrites(ch.id)
            .await
            .map_err(|e| e.to_string())?;
        let now = self.clock.now_unix_ms();
        let mut out = Vec::new();
        let mut after: Option<UserId> = None;
        while out.len() < VIEWER_CAP {
            let page = self
                .repo
                .page_members(gid, after, 500)
                .await
                .map_err(|e| e.to_string())?;
            for m in &page {
                let p = perms::channel_permissions(&guild, m, &roles, &overwrites, now);
                if p.has(Permissions::VIEW_CHANNEL) {
                    out.push(to_i64(m.user_id));
                }
            }
            match page.last() {
                Some(last) if page.len() == 500 => after = Some(last.user_id),
                _ => break,
            }
        }
        Ok(out)
    }
}

//! Web Push fan-out for new messages (LNG-NTF-002, LNG-NTF-003).
//!
//! A consumer of the event firehose looks at every `MESSAGE_CREATE`, finds the members
//! who have a push subscription and can see the channel, runs each through the
//! notification decision engine ([`sr_chat::social::notifications::decide`]: level, mutes,
//! suppress toggles, mentions, replies, DMs, thread follows) and sends the survivors a
//! small encrypted message. Messages to the same channel share a collapse key, so a busy
//! channel replaces its own pending notification instead of piling up.

use std::sync::Arc;

use futures::StreamExt;
use futures::stream::BoxStream;
use serde_json::json;
use sr_api::routes::messages::MessagesDeps;
use sr_api::routes::social::SocialDeps;
use sr_chat::social::notifications::{
    MentionKind, NotificationLevel, NotifyDecision, NotifyInput, NotifyReason, matches_keyword,
};
use sr_chat::{Message, MessageType};
use sr_core::{GuildId, MessageId, UserId};
use sr_db::PgPool;
use sr_events::{DomainEvent, FirehoseDedup};
use sr_push::{Notification, PushSender, PushStore, SendOutcome, Urgency};
use tokio::task::JoinHandle;

use crate::lag_bus::TapItem;
use crate::push_pg::{push_candidates, thread_followers};

/// Seconds a push service may hold an undelivered notification.
const TTL_SECS: u32 = 4 * 3600;
/// Characters of the message body sent in the push.
const BODY_CHARS: usize = 140;

/// Whether a user is at a live, non-idle session (the gateway answers; D-191).
pub trait SessionActivity: Send + Sync {
    fn has_active_session(&self, user: UserId) -> bool;
}

impl SessionActivity for sr_gateway::Gateway {
    fn has_active_session(&self, user: UserId) -> bool {
        sr_gateway::Gateway::has_active_session(self, user)
    }
}

/// Drop the targets that are using the app right now: no Web Push while any session of the
/// user is connected and not idle (D-191).
pub fn without_active(
    targets: Vec<(u64, NotifyDecision)>,
    activity: Option<&dyn SessionActivity>,
) -> Vec<(u64, NotifyDecision)> {
    match activity {
        Some(a) => targets
            .into_iter()
            .filter(|(u, _)| !a.has_active_session(UserId::new(*u)))
            .collect(),
        None => targets,
    }
}

pub struct PushNotifier {
    pub messages: Arc<MessagesDeps>,
    pub social: Arc<SocialDeps>,
    pub store: Arc<dyn PushStore>,
    pub sender: Arc<dyn PushSender>,
    pub pool: PgPool,
    /// Gateway view of who is active; `None` never suppresses.
    pub activity: Option<Arc<dyn SessionActivity>>,
}

/// Start the consumer. Subscribe to the tap before calling, so nothing published
/// afterwards is missed.
pub fn spawn(
    mut events: BoxStream<'static, TapItem>,
    notifier: Arc<PushNotifier>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut dedup = FirehoseDedup::new();
        while let Some(item) = events.next().await {
            // A lagged tap only loses notifications that are by now stale.
            let Ok(ev) = item else { continue };
            let wanted =
                matches!(ev, DomainEvent::MessageSent { .. }) || chapter_push(&ev).is_some();
            if wanted && dedup.is_new(&ev) {
                let n = notifier.clone();
                tokio::spawn(async move {
                    if let Err(e) = n.handle(&ev).await {
                        tracing::warn!(error = %e, "push fan-out failed");
                    }
                });
            }
        }
    })
}

/// A followed series got a chapter (LIB-SOC-007): the users to tell and the push payload, read from the
/// `SR_LIBRARY_UPDATE` `chapter_published` event the Library sends to followers with the bell on.
pub fn chapter_push(ev: &DomainEvent) -> Option<(Vec<UserId>, serde_json::Value)> {
    let DomainEvent::UserScoped {
        users,
        name,
        payload,
    } = ev
    else {
        return None;
    };
    if name != "SR_LIBRARY_UPDATE" || payload["type"] != "chapter_published" {
        return None;
    }
    let series = payload["series_id"].as_str()?;
    let chapter = payload["chapter_id"].as_str()?;
    let title = payload["series_title"]
        .as_str()
        .unwrap_or("A series you follow");
    let number = &payload["chapter_number"];
    let number = match number.as_f64() {
        Some(n) if n.fract() == 0.0 => format!("{n:.0}"),
        Some(n) => n.to_string(),
        None => String::new(),
    };
    let mut body = format!("Chapter {number} is out");
    if let Some(t) = payload["chapter_title"].as_str().filter(|t| !t.is_empty()) {
        body = format!("Chapter {number}: {t}");
    }
    Some((
        users.clone(),
        json!({
            "title": title,
            "body": body,
            "url": format!("/library/r/{chapter}"),
            "tag": format!("lib-{series}"),
        }),
    ))
}

/// Whether a message of this type can notify at all (system messages never do).
pub fn notifies(kind: MessageType) -> bool {
    matches!(kind, MessageType::Default | MessageType::Reply)
}

/// The mention facts of `msg` for `user` (`roles` are the user's role ids).
pub fn mention_kind(
    msg: &Message,
    user: UserId,
    roles: &[sr_core::RoleId],
    reply_author: Option<UserId>,
) -> MentionKind {
    MentionKind {
        direct: msg.mention_user_ids.contains(&user),
        reply: reply_author == Some(user),
        everyone: msg.mention_everyone,
        role: msg.mention_role_ids.iter().any(|r| roles.contains(r)),
    }
}

/// `title`/`body`/`url`/`tag` as the service worker expects them.
pub fn payload(
    reason: NotifyReason,
    author: &str,
    place: Option<&str>,
    content: &str,
    has_attachments: bool,
    url: &str,
    tag: &str,
) -> serde_json::Value {
    let title = match (reason, place) {
        (NotifyReason::DirectMessage, _) | (_, None) => author.to_owned(),
        (NotifyReason::Mention | NotifyReason::Everyone | NotifyReason::Role, Some(p)) => {
            format!("{author} mentioned you in #{p}")
        }
        (NotifyReason::Reply, Some(p)) => format!("{author} replied to you in #{p}"),
        (_, Some(p)) => format!("{author} in #{p}"),
    };
    let mut body: String = content.trim().chars().take(BODY_CHARS).collect();
    if content.trim().chars().count() > BODY_CHARS {
        body.push('\u{2026}');
    }
    if body.is_empty() && has_attachments {
        body = "Sent an attachment".to_owned();
    }
    json!({ "title": title, "body": body, "url": url, "tag": tag })
}

impl PushNotifier {
    /// Handle one `MessageSent` event.
    pub async fn handle(&self, ev: &DomainEvent) -> Result<(), String> {
        if let Some((users, body)) = chapter_push(ev) {
            return self.deliver_chapter(users, body).await;
        }
        let DomainEvent::MessageSent {
            guild,
            channel,
            message_id,
            author,
            ..
        } = ev
        else {
            return Ok(());
        };
        let Some(msg) = self
            .messages
            .repo
            .get_message(*channel, MessageId::new(*message_id))
            .await
            .map_err(|e| e.to_string())?
            .filter(Message::is_visible)
        else {
            return Ok(());
        };
        if !notifies(msg.kind) {
            return Ok(());
        }
        let candidates = push_candidates(
            &self.pool,
            guild.map(GuildId::get),
            channel.get(),
            author.get(),
        )
        .await
        .map_err(|e| e.to_string())?;
        if candidates.is_empty() {
            return Ok(());
        }
        let Some(ch) = self
            .messages
            .repo
            .get_channel(*channel)
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(());
        };
        let followers = if ch.kind.is_thread() {
            thread_followers(&self.pool, channel.get(), &candidates)
                .await
                .map_err(|e| e.to_string())?
        } else {
            Vec::new()
        };
        let reply_author = match &msg.reference {
            Some(r) if r.forward.is_none() => self
                .messages
                .repo
                .get_message(r.channel_id, r.message_id)
                .await
                .ok()
                .flatten()
                .map(|m| m.author_id),
            _ => None,
        };
        let guild_default = match guild {
            Some(g) => self
                .messages
                .repo
                .get_guild(*g)
                .await
                .map_err(|e| e.to_string())?
                .and_then(|g| {
                    NotificationLevel::from_code(i16::from(g.default_message_notifications))
                })
                .unwrap_or(NotificationLevel::All),
            None => NotificationLevel::All,
        };
        let category = ch.parent_id;

        let mut targets: Vec<(u64, NotifyDecision)> = Vec::new();
        for raw in candidates {
            let user = UserId::new(raw);
            // The member must be able to see the channel; roles come with the member row.
            let Ok(access) = self.messages.access(user, *channel).await else {
                continue;
            };
            if access.require_view().is_err() {
                continue;
            }
            let roles = match guild {
                Some(g) => self
                    .messages
                    .repo
                    .get_member(*g, user)
                    .await
                    .ok()
                    .flatten()
                    .map(|m| m.role_ids)
                    .unwrap_or_default(),
                None => Vec::new(),
            };
            let keywords = self
                .social
                .service
                .notification_keywords(user)
                .await
                .unwrap_or_default();
            let input = NotifyInput {
                kind: mention_kind(&msg, user, &roles, reply_author),
                is_dm: guild.is_none(),
                followed_thread: followers.contains(&raw),
                keyword_hit: matches_keyword(&msg.content, &keywords),
            };
            let decision = self
                .social
                .service
                .decide_notification(user, *guild, *channel, category, guild_default, input)
                .await
                .map_err(|e| e.to_string())?;
            if decision.push {
                targets.push((raw, decision));
            }
        }
        let targets = without_active(targets, self.activity.as_deref());
        if targets.is_empty() {
            return Ok(());
        }
        self.deliver(&msg, &ch, *guild, targets).await
    }

    /// Web Push for a new chapter to the followers with the bell on, except those using the app right
    /// now (D-191). Messages of one series share a collapse key.
    async fn deliver_chapter(
        &self,
        users: Vec<UserId>,
        body: serde_json::Value,
    ) -> Result<(), String> {
        let ids: Vec<u64> = users
            .into_iter()
            .filter(|u| {
                !self
                    .activity
                    .as_deref()
                    .is_some_and(|a| a.has_active_session(*u))
            })
            .map(UserId::get)
            .collect();
        if ids.is_empty() {
            return Ok(());
        }
        let tag = body["tag"].as_str().unwrap_or("lib").to_owned();
        let subs = self
            .store
            .for_users(&ids)
            .await
            .map_err(|e| e.to_string())?;
        for s in subs {
            let n = Notification {
                payload: body.to_string().into_bytes(),
                collapse_key: Some(tag.clone()),
                urgency: Urgency::Normal,
                ttl_secs: TTL_SECS,
            };
            match self.sender.send(&s.sub, &n).await {
                Ok(SendOutcome::Gone) => {
                    let _ = self.store.delete_endpoint(&s.sub.endpoint).await;
                }
                Ok(SendOutcome::Failed(status)) => {
                    tracing::debug!(status, "push service refused a notification");
                }
                Ok(SendOutcome::Delivered) => {}
                Err(e) => tracing::debug!(error = %e, "push delivery failed"),
            }
        }
        Ok(())
    }

    async fn deliver(
        &self,
        msg: &Message,
        ch: &sr_chat::Channel,
        guild: Option<GuildId>,
        targets: Vec<(u64, NotifyDecision)>,
    ) -> Result<(), String> {
        let author_name = self
            .social
            .summaries([msg.author_id])
            .await
            .ok()
            .and_then(|m| m.get(&msg.author_id).cloned())
            .map_or_else(
                || "Someone".to_owned(),
                |u| u.display_name.unwrap_or(u.username),
            );
        let url = match guild {
            Some(g) => format!("/channels/{}/{}", g.get(), ch.id.get()),
            None => format!("/channels/@me/{}", ch.id.get()),
        };
        let tag = format!("ch-{}", ch.id.get());
        let ids: Vec<u64> = targets.iter().map(|(u, _)| *u).collect();
        let subs = self
            .store
            .for_users(&ids)
            .await
            .map_err(|e| e.to_string())?;
        for s in subs {
            let Some((_, d)) = targets.iter().find(|(u, _)| *u == s.user_id) else {
                continue;
            };
            let body = payload(
                d.reason.unwrap_or(NotifyReason::AllMessages),
                &author_name,
                if guild.is_some() {
                    ch.name.as_deref()
                } else {
                    None
                },
                &msg.content,
                !msg.attachment_ids.is_empty(),
                &url,
                &tag,
            );
            let high = matches!(
                d.reason,
                Some(NotifyReason::Mention | NotifyReason::Reply | NotifyReason::DirectMessage)
            );
            let n = Notification {
                payload: body.to_string().into_bytes(),
                collapse_key: Some(tag.clone()),
                urgency: if high { Urgency::High } else { Urgency::Normal },
                ttl_secs: TTL_SECS,
            };
            match self.sender.send(&s.sub, &n).await {
                Ok(SendOutcome::Gone) => {
                    let _ = self.store.delete_endpoint(&s.sub.endpoint).await;
                }
                Ok(SendOutcome::Failed(status)) => {
                    tracing::debug!(status, "push service refused a notification");
                }
                Ok(SendOutcome::Delivered) => {}
                Err(e) => tracing::debug!(error = %e, "push delivery failed"),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg() -> Message {
        let mut m = crate::push_notify::tests_support::blank_message();
        m.mention_user_ids = vec![UserId::new(5)];
        m.mention_role_ids = vec![sr_core::RoleId::new(9)];
        m
    }

    #[test]
    fn mention_kind_reads_users_roles_and_replies() {
        let m = msg();
        let k = mention_kind(&m, UserId::new(5), &[], None);
        assert!(k.direct && !k.reply && !k.role && !k.everyone);
        let k = mention_kind(
            &m,
            UserId::new(6),
            &[sr_core::RoleId::new(9)],
            Some(UserId::new(6)),
        );
        assert!(!k.direct && k.reply && k.role);
    }

    struct Fake(Vec<u64>);
    impl SessionActivity for Fake {
        fn has_active_session(&self, user: UserId) -> bool {
            self.0.contains(&user.get())
        }
    }

    #[test]
    fn active_sessions_suppress_push_and_idle_or_absent_ones_do_not() {
        let d = NotifyDecision {
            notify: true,
            push: true,
            reason: Some(NotifyReason::AllMessages),
        };
        let targets = vec![(1, d), (2, d), (3, d)];
        // 1 is using the app; 2 is idle or gone; 3 has no gateway session.
        let kept = without_active(targets.clone(), Some(&Fake(vec![1])));
        assert_eq!(kept.iter().map(|t| t.0).collect::<Vec<_>>(), vec![2, 3]);
        assert_eq!(without_active(targets, None).len(), 3);
    }
    #[test]
    fn a_published_chapter_becomes_a_push_for_its_followers() {
        let ev = |t: &str| DomainEvent::UserScoped {
            users: vec![UserId::new(4), UserId::new(5)],
            name: "SR_LIBRARY_UPDATE".into(),
            payload: json!({"type": t, "series_id": "9", "series_title": "Frieren",
                "chapter_id": "77", "chapter_number": 36.0, "chapter_title": "Spring"}),
        };
        let (users, body) = chapter_push(&ev("chapter_published")).unwrap();
        assert_eq!(users, vec![UserId::new(4), UserId::new(5)]);
        assert_eq!(body["title"], "Frieren");
        assert_eq!(body["body"], "Chapter 36: Spring");
        assert_eq!(body["url"], "/library/r/77");
        assert_eq!(body["tag"], "lib-9");
        // Progress syncs and other updates never push.
        assert!(chapter_push(&ev("progress")).is_none());
        let other = DomainEvent::UserScoped {
            users: vec![],
            name: "SR_CINEMA_UPDATE".into(),
            payload: json!({"type": "chapter_published"}),
        };
        assert!(chapter_push(&other).is_none());
    }

    #[test]
    fn system_messages_never_notify() {
        assert!(notifies(MessageType::Default));
        assert!(notifies(MessageType::Reply));
        assert!(!notifies(MessageType::UserJoin));
        assert!(!notifies(MessageType::ChannelPinnedMessage));
    }

    #[test]
    fn payload_titles_follow_the_reason() {
        let p = |r, place| payload(r, "Ana", place, "hello", false, "/channels/1/2", "ch-2");
        assert_eq!(
            p(NotifyReason::AllMessages, Some("general"))["title"],
            "Ana in #general"
        );
        assert_eq!(
            p(NotifyReason::Mention, Some("general"))["title"],
            "Ana mentioned you in #general"
        );
        assert_eq!(
            p(NotifyReason::Reply, Some("general"))["title"],
            "Ana replied to you in #general"
        );
        assert_eq!(p(NotifyReason::DirectMessage, None)["title"], "Ana");
        assert_eq!(p(NotifyReason::AllMessages, None)["body"], "hello");
        assert_eq!(p(NotifyReason::AllMessages, None)["url"], "/channels/1/2");
        let long = "x".repeat(300);
        let v = payload(NotifyReason::AllMessages, "A", None, &long, false, "/", "t");
        assert_eq!(v["body"].as_str().unwrap().chars().count(), BODY_CHARS + 1);
        let v = payload(NotifyReason::AllMessages, "A", None, "  ", true, "/", "t");
        assert_eq!(v["body"], "Sent an attachment");
    }
}

#[cfg(test)]
pub(crate) mod tests_support {
    use sr_chat::{Message, MessageType, Visibility};
    use sr_core::{ChannelId, MessageId, UserId};

    pub fn blank_message() -> Message {
        Message {
            id: MessageId::new(1),
            channel_id: ChannelId::new(2),
            guild_id: None,
            author_id: UserId::new(3),
            kind: MessageType::Default,
            content: String::new(),
            flags: 0,
            edited_at_ms: None,
            reference: None,
            attachment_ids: Vec::new(),
            embeds: Vec::new(),
            components: Vec::new(),
            sticker_ids: Vec::new(),
            poll: None,
            mention_everyone: false,
            mention_user_ids: Vec::new(),
            mention_role_ids: Vec::new(),
            mention_channel_ids: Vec::new(),
            pinned: false,
            visibility: Visibility::Visible,
            deleted_at_ms: None,
            purge_after_ms: None,
            hold_id: None,
        }
    }
}

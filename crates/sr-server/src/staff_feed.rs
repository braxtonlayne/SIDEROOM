//! The Staff Server's system user: bot-style cards in the staff channels, the appointment
//! DM, announcement DMs, and the task that turns platform events into all of those
//! (ADM-STF-007, ADM-ALR-001/002/003, ADM-STF-005).
//!
//! Everything is posted as the SIDE ROOM system user (id 1) through the ordinary message
//! stack, so cards are real messages: they have a history, render like any other message and
//! reach clients through the gateway. Buttons carry `custom_id`s of the form `sr:<action>:<id>`
//! which `POST /admin/card-actions` runs with the presser's own platform permissions.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use futures::StreamExt;
use serde_json::json;
use sqlx::PgPool;
use sr_admin::{
    AdminService, Announcement, AnnouncementSeverity, SYSTEM_USER_ID, StaffNotifier, StaffOffer,
};
use sr_api::routes::{
    auth::mail::{Mailer, OutgoingMail},
    messages::{MessagesDeps, render::event_payload},
};
use sr_chat::{
    SendMessage,
    message::{Component, Embed},
    social::repo::DmRepository,
};
use sr_core::{ChannelId, UserId};
use sr_db::social_repo::PgSocialRepo;
use sr_events::{DomainEvent, EventBus, StaffAlertKind, StaffFeedKind, Topic};

/// Embed colours.
const RED: u32 = 0xE74C3C;
const AMBER: u32 = 0xF1C40F;
const BLUE: u32 = 0x3498DB;
const GREEN: u32 = 0x2ECC71;

/// Button styles (Discord).
const SECONDARY: u8 = 2;
const SUCCESS: u8 = 3;
const DANGER: u8 = 4;
const LINK: u8 = 5;

/// At most one alert email per kind in this window (ADM-ALR-003).
const MAIL_WINDOW: Duration = Duration::from_secs(30 * 60);
/// Pause between announcement DMs, so a big installation is not hit all at once.
const DM_PACE: Duration = Duration::from_millis(40);

/// A message the system user posts: text, one embed, and one row of buttons.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Card {
    pub content: String,
    pub embed: Option<Embed>,
    pub buttons: Vec<Component>,
}

impl Card {
    fn into_send(self) -> SendMessage {
        let components = if self.buttons.is_empty() {
            Vec::new()
        } else {
            vec![Component {
                kind: 1,
                components: self.buttons,
                ..Component::default()
            }]
        };
        SendMessage {
            content: self.content,
            embeds: self.embed.into_iter().collect(),
            components,
            allowed_mentions: Some(sr_chat::message_rules::AllowedMentions::default()),
            ..SendMessage::default()
        }
    }
}

fn button(style: u8, label: &str, custom_id: String) -> Component {
    Component {
        kind: 2,
        style: Some(style),
        label: Some(label.to_owned()),
        custom_id: Some(custom_id),
        ..Component::default()
    }
}

fn link_button(label: &str, url: String) -> Component {
    Component {
        kind: 2,
        style: Some(LINK),
        label: Some(label.to_owned()),
        url: Some(url),
        ..Component::default()
    }
}

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('\u{2026}');
        out
    }
}

/// Human title, colour and the admin page an alert links to.
fn alert_meta(kind: StaffAlertKind) -> (&'static str, u32, &'static str) {
    match kind {
        StaffAlertKind::BannedPersonAttempt => {
            ("Banned person tried to register", AMBER, "/admin/bans")
        }
        StaffAlertKind::InviterSuspicion => {
            ("Inviter suspicion raised", AMBER, "/admin/invite-graph")
        }
        StaffAlertKind::InviterEscalated => {
            ("Inviter suspicion escalated", RED, "/admin/invite-graph")
        }
        StaffAlertKind::StaffRevoked => ("Staff access ended", BLUE, "/admin/staff"),
        StaffAlertKind::AccessLogChainBroken => {
            ("Access log chain is broken", RED, "/admin/access-log")
        }
        StaffAlertKind::PaymentAttention => ("Payment needs attention", AMBER, "/admin/donations"),
        StaffAlertKind::DiskLow => ("Disk is filling up", RED, "/admin/system"),
        StaffAlertKind::BackupFailed => ("Backup problem", RED, "/admin/system"),
        StaffAlertKind::TranscodeFailed => ("Transcode job failed", AMBER, "/admin/transcoding"),
    }
}

/// The card for an alert: what happened, Acknowledge and a way into the admin page.
pub fn alert_card(kind: StaffAlertKind, detail: &str, alert_id: i64, base_url: &str) -> Card {
    let (title, color, path) = alert_meta(kind);
    let buttons = vec![
        button(SECONDARY, "Acknowledge", format!("sr:ack:{alert_id}")),
        link_button("Open in Admin", format!("{base_url}{path}")),
    ];
    Card {
        content: String::new(),
        embed: Some(Embed {
            title: Some(title.to_owned()),
            kind: Some("rich".into()),
            description: Some(clip(detail, 1000)),
            color,
            ..Embed::default()
        }),
        buttons,
    }
}

/// The card for a feed (`#new-members`, `#donations`).
pub fn feed_card(
    feed: StaffFeedKind,
    title: &str,
    detail: &str,
    link: Option<&str>,
    base_url: &str,
) -> Card {
    let color = match feed {
        StaffFeedKind::NewMember => BLUE,
        StaffFeedKind::Donation => GREEN,
    };
    Card {
        content: String::new(),
        embed: Some(Embed {
            title: Some(clip(title, 200)),
            kind: Some("rich".into()),
            description: Some(clip(detail, 1000)),
            color,
            ..Embed::default()
        }),
        buttons: link
            .filter(|l| l.starts_with('/'))
            .map(|l| {
                let label = match feed {
                    StaffFeedKind::NewMember => "Open in invite graph",
                    StaffFeedKind::Donation => "Open invoice",
                };
                link_button(label, format!("{base_url}{l}"))
            })
            .into_iter()
            .collect(),
    }
}

/// The system DM that offers an appointment (doc 07 section 4).
pub fn offer_card(offer: &StaffOffer) -> Card {
    let roles = offer.role_names.join(", ");
    let reason = if offer.reason.is_empty() {
        String::new()
    } else {
        format!("\n\nWhy: {}", clip(&offer.reason, 400))
    };
    Card {
        content: "You've been invited to join SIDE ROOM Staff.".to_owned(),
        embed: Some(Embed {
            title: Some("Staff appointment".into()),
            kind: Some("rich".into()),
            description: Some(format!(
                "@{} would like you to join the staff as {roles}.{reason}\n\nYou join only if you accept. The offer expires after seven days.",
                offer.offered_by_name
            )),
            color: BLUE,
            ..Embed::default()
        }),
        buttons: vec![
            button(SUCCESS, "Accept", format!("sr:offer-accept:{}", offer.id)),
            button(DANGER, "Decline", format!("sr:offer-decline:{}", offer.id)),
        ],
    }
}

/// An announcement as a DM.
pub fn announcement_card(a: &Announcement) -> Card {
    let color = match a.severity {
        AnnouncementSeverity::Info => BLUE,
        AnnouncementSeverity::Warning => AMBER,
        AnnouncementSeverity::Critical => RED,
    };
    Card {
        content: String::new(),
        embed: Some(Embed {
            title: Some(clip(&a.title, 200)),
            kind: Some("rich".into()),
            description: Some(clip(&a.body, 4000)),
            color,
            ..Embed::default()
        }),
        buttons: Vec::new(),
    }
}

/// Used percent of a disk, rounded down. `None` for an empty or unreadable one.
pub fn used_percent(total: u64, available: u64) -> Option<u8> {
    if total == 0 {
        return None;
    }
    let used = total.saturating_sub(available);
    u8::try_from(u128::from(used) * 100 / u128::from(total)).ok()
}

/// "12.3 GiB".
pub fn gib(bytes: u64) -> String {
    format!("{:.1} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

/// The system user speaking in the Staff Server.
pub struct StaffVoice {
    pool: PgPool,
    messages: Arc<MessagesDeps>,
    base_url: String,
    member: tokio::sync::OnceCell<()>,
}

impl StaffVoice {
    pub fn new(pool: PgPool, messages: Arc<MessagesDeps>, public_url: &str) -> Self {
        Self {
            pool,
            messages,
            base_url: public_url.trim_end_matches('/').to_owned(),
            member: tokio::sync::OnceCell::new(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn system() -> UserId {
        UserId::new(SYSTEM_USER_ID as u64)
    }

    /// The Staff Server, if bootstrap created one.
    pub async fn staff_guild(&self) -> Option<i64> {
        sqlx::query_scalar("SELECT id FROM guilds WHERE is_staff_guild")
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten()
    }

    /// A text channel of the Staff Server by name.
    async fn channel(&self, name: &str) -> Option<ChannelId> {
        let id: Option<i64> = sqlx::query_scalar(
            "SELECT c.id FROM channels c JOIN guilds g ON g.id = c.guild_id \
             WHERE g.is_staff_guild AND c.type = 0 AND c.name = $1 ORDER BY c.id LIMIT 1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten();
        id.and_then(|i| u64::try_from(i).ok()).map(ChannelId::new)
    }

    /// The system user must be a member of the Staff Server to post there. Done once.
    async fn ensure_member(&self) {
        self.member
            .get_or_init(|| async {
                let r = sqlx::query(
                    "INSERT INTO guild_members (guild_id, user_id) \
                     SELECT id, $1 FROM guilds WHERE is_staff_guild \
                     ON CONFLICT (guild_id, user_id) DO NOTHING",
                )
                .bind(SYSTEM_USER_ID)
                .execute(&self.pool)
                .await;
                if let Err(e) = r {
                    tracing::warn!(error = %e, "could not add the system user to the Staff Server");
                }
            })
            .await;
    }

    async fn deliver(&self, channel: ChannelId, card: Card) -> Option<i64> {
        let author = Self::system();
        let plan = match self.messages.access(author, channel).await {
            Ok(acc) => self.messages.plan_dm_announce(&acc.channel, author).await,
            Err(e) => {
                tracing::warn!(error = ?e, channel = channel.get(), "system user cannot post here");
                return None;
            }
        };
        let msg = match self.messages.send(author, channel, card.into_send()).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = ?e, channel = channel.get(), "staff card was refused");
                return None;
            }
        };
        let data = match event_payload(&self.messages, &msg).await {
            Ok(d) => d,
            Err(_) => json!({}),
        };
        self.messages.announce_dm(channel, &plan).await;
        self.messages
            .publish(DomainEvent::MessageSent {
                guild: msg.guild_id,
                channel,
                message_id: msg.id.get(),
                author,
                data,
            })
            .await;
        i64::try_from(msg.id.get()).ok()
    }

    /// Post a card to a Staff Server channel (`alerts`, `new-members`, ...). `false` when the
    /// channel does not exist (an installation without a Staff Server) or posting failed.
    pub async fn post(&self, channel_name: &str, card: Card) -> bool {
        self.ensure_member().await;
        let Some(channel) = self.channel(channel_name).await else {
            tracing::debug!(
                channel = channel_name,
                "no such Staff Server channel; card dropped"
            );
            return false;
        };
        self.deliver(channel, card).await.is_some()
    }

    /// DM `to` from the system user. Returns the DM channel and message ids.
    pub async fn dm(&self, to: i64, card: Card) -> Option<(i64, i64)> {
        let social = PgSocialRepo::new(self.pool.clone());
        let target = UserId::new(u64::try_from(to).ok()?);
        let channel = match social.find_dm(Self::system(), target).await {
            Ok(Some(c)) => c.id,
            Ok(None) => {
                let id = ChannelId::new(self.messages.next_id().get());
                if let Err(e) = social.insert_dm(id, Self::system(), target).await {
                    tracing::warn!(error = %e, "could not open a system DM");
                    return None;
                }
                id
            }
            Err(e) => {
                tracing::warn!(error = %e, "could not look up a system DM");
                return None;
            }
        };
        let message = self.deliver(channel, card).await?;
        Some((i64::try_from(channel.get()).ok()?, message))
    }

    /// Email address of the Staff Server's owner.
    pub async fn owner_email(&self) -> Option<String> {
        sqlx::query_scalar(
            "SELECT a.email::text FROM guilds g JOIN accounts a ON a.id = g.owner_id \
             WHERE g.is_staff_guild AND a.state = 'active'",
        )
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
    }
}

#[async_trait]
impl StaffNotifier for StaffVoice {
    async fn appointment_offer(&self, offer: &StaffOffer) -> Option<(i64, i64)> {
        self.dm(offer.account_id, offer_card(offer)).await
    }

    async fn staff_notice(&self, text: &str) {
        self.post(
            "staff-announcements",
            Card {
                content: clip(text, 1900),
                ..Card::default()
            },
        )
        .await;
    }

    fn broadcast_announcement(&self, a: &Announcement) {
        let (pool, messages, base) = (
            self.pool.clone(),
            self.messages.clone(),
            self.base_url.clone(),
        );
        let a = a.clone();
        tokio::spawn(async move {
            let voice = StaffVoice::new(pool.clone(), messages, &base);
            let ids: Vec<i64> = sqlx::query_scalar(
                "SELECT id FROM accounts WHERE state = 'active' AND id <> $1 AND NOT is_bot ORDER BY id",
            )
            .bind(SYSTEM_USER_ID)
            .fetch_all(&pool)
            .await
            .unwrap_or_default();
            let mut sent = 0_usize;
            for id in &ids {
                if voice.dm(*id, announcement_card(&a)).await.is_some() {
                    sent += 1;
                }
                tokio::time::sleep(DM_PACE).await;
            }
            tracing::info!(
                announcement = a.id,
                sent,
                of = ids.len(),
                "announcement DMs delivered"
            );
        });
    }
}

/// Remembers when each alert kind was last mailed.
#[derive(Default)]
pub struct MailThrottle(Mutex<HashMap<&'static str, Instant>>);

impl MailThrottle {
    /// `true` (and records the time) when a mail of `kind` may go out now.
    pub fn allow(&self, kind: &'static str) -> bool {
        let mut m = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        match m.get(kind) {
            Some(at) if now.duration_since(*at) < MAIL_WINDOW => false,
            _ => {
                m.insert(kind, now);
                true
            }
        }
    }
}

/// Everything the consumer needs.
pub struct StaffFeed {
    pub voice: Arc<StaffVoice>,
    pub admin: AdminService,
    pub mailer: Arc<dyn Mailer>,
    pub bus: Arc<dyn EventBus>,
    pub throttle: MailThrottle,
}

impl StaffFeed {
    /// One alert: store it, post the card, maybe mail the Owner.
    pub async fn on_alert(&self, kind: StaffAlertKind, detail: &str) {
        let id = match self.admin.record_alert(kind.as_str(), detail).await {
            Ok(r) => r.id,
            Err(e) => {
                tracing::warn!(error = %e, "could not store a staff alert");
                0
            }
        };
        let card = alert_card(kind, detail, id, self.voice.base_url());
        self.voice.post("alerts", card).await;
        self.mail_owner(kind, detail).await;
    }

    async fn mail_owner(&self, kind: StaffAlertKind, detail: &str) {
        let Ok(settings) = self.admin.alert_settings().await else {
            return;
        };
        if !settings.email_to_owner || !self.throttle.allow(kind.as_str()) {
            return;
        }
        let Some(to) = self.voice.owner_email().await else {
            return;
        };
        let (title, _, path) = alert_meta(kind);
        let link = format!("{}{path}", self.voice.base_url());
        let text = format!("{title}\n\n{detail}\n\nOpen the admin page: {link}\n");
        let html = format!(
            "<p><strong>{}</strong></p><p>{}</p><p><a href=\"{}\">Open the admin page</a></p>",
            escape(title),
            escape(detail),
            escape(&link)
        );
        let mail = OutgoingMail {
            to,
            subject: format!("[SIDE ROOM] {title}"),
            text,
            html,
        };
        if let Err(e) = self.mailer.send(mail).await {
            tracing::warn!(error = %e, "could not email a staff alert");
        }
    }

    /// A feed card.
    pub async fn on_feed(
        &self,
        feed: StaffFeedKind,
        title: &str,
        detail: &str,
        link: Option<&str>,
    ) {
        let card = feed_card(feed, title, detail, link, self.voice.base_url());
        self.voice.post(feed.channel_name(), card).await;
    }

    /// Someone left (or was removed from) a guild. In the Staff Server that ends their
    /// staff status at once: the database closed the appointment, now tell their sessions
    /// and the staff (ADM-STF-005).
    pub async fn on_member_left(&self, guild: i64, user: i64) {
        if self.voice.staff_guild().await != Some(guild) {
            return;
        }
        let name: Option<String> = sqlx::query_scalar(
            "SELECT a.username::text FROM staff_appointments ap JOIN accounts a ON a.id = ap.account_id \
             WHERE ap.account_id = $1 AND ap.revoke_reason = 'left or was removed from the Staff Server' \
               AND ap.revoked_at > now() - interval '1 minute' \
             ORDER BY ap.revoked_at DESC LIMIT 1",
        )
        .bind(user)
        .fetch_optional(&self.voice.pool)
        .await
        .ok()
        .flatten();
        let Some(name) = name else {
            return;
        };
        self.admin.push_revoked(user).await;
        let ev = DomainEvent::StaffAlert {
            kind: StaffAlertKind::StaffRevoked,
            detail: format!("{name} left or was removed from the Staff Server."),
        };
        if let Err(e) = self.bus.publish(Topic::Platform, ev).await {
            tracing::warn!(error = %e, "could not publish a staff revocation");
        }
    }

    /// Dispatch one platform event.
    pub async fn handle(&self, ev: DomainEvent) {
        match ev {
            DomainEvent::StaffAlert { kind, detail } => self.on_alert(kind, &detail).await,
            DomainEvent::StaffFeed {
                feed,
                title,
                detail,
                link,
            } => self.on_feed(feed, &title, &detail, link.as_deref()).await,
            DomainEvent::MemberLeft { guild, user } => {
                if let (Ok(g), Ok(u)) = (i64::try_from(guild.get()), i64::try_from(user.get())) {
                    self.on_member_left(g, u).await;
                }
            }
            _ => {}
        }
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Start the consumer: every platform event that concerns staff becomes a card (and maybe an
/// email). Returns the task handle; the task ends when the bus closes.
pub fn spawn(feed: Arc<StaffFeed>) -> tokio::task::JoinHandle<()> {
    let mut events = feed.bus.subscribe(Topic::Platform);
    tokio::spawn(async move {
        while let Some(ev) = events.next().await {
            let feed = feed.clone();
            // One task per event: posting and mailing must not hold up the next alert.
            tokio::spawn(async move { feed.handle(ev).await });
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(c: &Card) -> Vec<(String, Option<String>, Option<String>)> {
        c.buttons
            .iter()
            .map(|b| {
                (
                    b.label.clone().unwrap_or_default(),
                    b.custom_id.clone(),
                    b.url.clone(),
                )
            })
            .collect()
    }

    #[test]
    fn alert_cards_carry_ack_and_a_link() {
        let c = alert_card(
            StaffAlertKind::DiskLow,
            "Disk 91% full",
            77,
            "https://sr.test",
        );
        let b = labels(&c);
        assert_eq!(b[0], ("Acknowledge".into(), Some("sr:ack:77".into()), None));
        assert_eq!(
            b[1],
            (
                "Open in Admin".into(),
                None,
                Some("https://sr.test/admin/system".into())
            )
        );
        assert_eq!(b.len(), 2);
        let e = c.embed.unwrap();
        assert_eq!(e.title.as_deref(), Some("Disk is filling up"));
        assert_eq!(e.color, RED);
        // The message is never a ping.
        assert!(c.content.is_empty());
    }

    #[test]
    fn feed_cards_link_only_to_admin_paths() {
        let c = feed_card(
            StaffFeedKind::NewMember,
            "New member",
            "@new joined, invited by @issuer.",
            Some("/admin/invite-graph?account=5"),
            "https://sr.test",
        );
        assert_eq!(
            c.buttons[0].url.as_deref(),
            Some("https://sr.test/admin/invite-graph?account=5")
        );
        // A link that is not an admin path on this server is dropped.
        let c = feed_card(
            StaffFeedKind::Donation,
            "Donation confirmed",
            "x",
            Some("https://evil.example/"),
            "https://sr.test",
        );
        assert!(c.buttons.is_empty());
        assert_eq!(StaffFeedKind::Donation.channel_name(), "donations");
        assert_eq!(StaffFeedKind::NewMember.channel_name(), "new-members");
    }

    #[test]
    fn the_offer_dm_has_accept_and_decline() {
        let offer = StaffOffer {
            id: 31,
            account_id: 2,
            username: "newbie".into(),
            offered_by: 1,
            offered_by_name: "owner".into(),
            role_ids: vec!["5".into()],
            role_names: vec!["Moderator".into(), "Support".into()],
            reason: "great help".into(),
            state: sr_admin::OfferState::Pending,
            created_at_ms: 0,
            expires_at_ms: 1,
            responded_at_ms: None,
            dm_channel_id: None,
            dm_message_id: None,
        };
        let c = offer_card(&offer);
        assert_eq!(c.content, "You've been invited to join SIDE ROOM Staff.");
        let ids: Vec<_> = labels(&c).into_iter().map(|b| b.1).collect();
        assert_eq!(
            ids,
            [
                Some("sr:offer-accept:31".into()),
                Some("sr:offer-decline:31".into())
            ]
        );
        let d = c.embed.unwrap().description.unwrap();
        assert!(
            d.contains("@owner") && d.contains("Moderator, Support") && d.contains("great help")
        );
    }

    #[test]
    fn disk_math_and_clipping() {
        assert_eq!(used_percent(100, 20), Some(80));
        assert_eq!(used_percent(100, 100), Some(0));
        assert_eq!(used_percent(0, 0), None);
        assert_eq!(used_percent(1000, 199), Some(80));
        assert_eq!(gib(3 * 1024 * 1024 * 1024), "3.0 GiB");
        assert_eq!(clip("abcdef", 4), "abc\u{2026}");
        assert_eq!(clip("abc", 4), "abc");
    }

    #[test]
    fn mail_is_throttled_per_kind() {
        let t = MailThrottle::default();
        assert!(t.allow("disk_low"));
        assert!(!t.allow("disk_low"));
        assert!(t.allow("backup_failed"));
    }
}

//! Wiring of the feature modules of `sr-userfeat` (docs/inventory/09-feature-modules.md): the system user as a
//! [`Poster`] over the message stack, the scheduler job (`userfeat.tick`), the leveling hook on stored messages,
//! and the watch party announcer hook.
//!
//! * Posts by the platform (feed entries, birthday and library cards, suggestion cards) are made by the SIDE ROOM
//!   system user (id 1) through the ordinary message service, so they are real messages with mention rules,
//!   history and gateway events. The system user joins a server the first time a module speaks there, and needs
//!   `Send Messages` in the channel like any bot; a role ping works when the role is mentionable.
//! * A scheduled message is sent as its author through the same service, so slowmode, channel permissions and
//!   AutoMod are checked at send time.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::PgPool;
use sr_admin::SYSTEM_USER_ID;
use sr_api::routes::guilds::backend::GuildBackend;
use sr_api::routes::messages::{MessagesDeps, render::event_payload, store::EmojiKey};
use sr_api::routes::settings::{AutoModGate, MessageCheck};
use sr_chat::message::{Embed, EmbedField, EmbedFooter, EmbedMedia};
use sr_chat::message_hooks::MessageHooks;
use sr_chat::message_rules::AllowedMentions;
use sr_chat::social::repo::DmRepository;
use sr_chat::{ChatError, Message, MessageType, SendMessage};
use sr_core::{ChannelId, GuildId, MessageId, RoleId, UserId};
use sr_db::DbResult;
use sr_db::jobs::{self, Job};
use sr_db::runner::{HandlerError, JobRunner};
use sr_db::social_repo::PgSocialRepo;
use sr_events::DomainEvent;
use sr_net::SafeClient;
use sr_userfeat::announcers::PartyAnnouncement;
use sr_userfeat::features::LEVELING_ID;
use sr_userfeat::leveling::{pick_xp, roles_between};
use sr_userfeat::tick::Ticker;
use sr_userfeat::{Card, Poster, SendFailure, UserFeatures};

pub const TICK_KIND: &str = "userfeat.tick";
/// Reserved singleton id of the scheduler job.
pub const TICK_JOB_ID: i64 = 760;
const TICK_INTERVAL: Duration = Duration::from_secs(15);

/// Filled once the message stack exists; the hooks that must run while it is being built hold the cell.
pub type PosterCell = Arc<OnceLock<Arc<dyn Poster>>>;

/// The system user speaking, and users speaking later.
pub struct SystemPoster {
    pool: PgPool,
    messages: Arc<MessagesDeps>,
    guilds: Arc<dyn GuildBackend>,
    automod: Option<Arc<dyn AutoModGate>>,
    base_url: String,
}

fn system() -> UserId {
    UserId::new(SYSTEM_USER_ID as u64)
}

fn cid(v: i64) -> ChannelId {
    ChannelId::new(u64::try_from(v).unwrap_or(0))
}

fn uid(v: i64) -> UserId {
    UserId::new(u64::try_from(v).unwrap_or(0))
}

fn rid(v: i64) -> RoleId {
    RoleId::new(u64::try_from(v).unwrap_or(0))
}

fn raw(v: u64) -> i64 {
    i64::try_from(v).unwrap_or(0)
}

/// A web address for the embed: relative paths get the public URL, anything but http(s) is dropped.
fn absolute(base: &str, url: Option<&str>) -> Option<String> {
    let u = url?.trim();
    if u.starts_with('/') {
        Some(format!("{base}{u}"))
    } else if u.starts_with("https://") || u.starts_with("http://") {
        Some(u.to_owned())
    } else {
        None
    }
}

/// The message a card becomes.
pub fn card_to_send(card: &Card, base: &str) -> SendMessage {
    let has_embed = card.title.is_some()
        || card.description.is_some()
        || !card.fields.is_empty()
        || card.thumbnail.is_some();
    let embed = has_embed.then(|| Embed {
        title: card.title.clone(),
        kind: Some("rich".into()),
        description: card.description.clone(),
        url: absolute(base, card.url.as_deref()),
        color: card.color,
        footer: card.footer.clone().map(|text| EmbedFooter {
            text,
            icon_url: None,
        }),
        thumbnail: card
            .thumbnail
            .as_deref()
            .and_then(|t| absolute(base, Some(t)))
            .map(|url| EmbedMedia {
                url: Some(url),
                ..EmbedMedia::default()
            }),
        fields: card
            .fields
            .iter()
            .map(|f| EmbedField {
                name: f.name.clone(),
                value: f.value.clone(),
                inline: f.inline,
            })
            .collect(),
        ..Embed::default()
    });
    SendMessage {
        content: card.content.clone(),
        embeds: embed.into_iter().collect(),
        allowed_mentions: Some(AllowedMentions {
            users: card.ping_users.iter().map(|u| uid(*u)).collect(),
            roles: card.ping_roles.iter().map(|r| rid(*r)).collect(),
            ..AllowedMentions::default()
        }),
        ..SendMessage::default()
    }
}

/// Whether a failed send is worth trying again.
fn chat_failure(e: &ChatError) -> SendFailure {
    match e {
        ChatError::Slowmode { .. } | ChatError::Repo(_) => SendFailure::Retry(e.to_string()),
        ChatError::MissingAccess | ChatError::MissingPermissions => SendFailure::Permanent(
            "You no longer have permission to send messages in that channel.".into(),
        ),
        ChatError::UnknownChannel => {
            SendFailure::Permanent("That channel no longer exists.".into())
        }
        other => SendFailure::Permanent(other.to_string()),
    }
}

impl SystemPoster {
    pub fn new(
        pool: PgPool,
        messages: Arc<MessagesDeps>,
        guilds: Arc<dyn GuildBackend>,
        automod: Option<Arc<dyn AutoModGate>>,
        public_url: &str,
    ) -> Self {
        Self {
            pool,
            messages,
            guilds,
            automod,
            base_url: public_url.trim_end_matches('/').to_owned(),
        }
    }

    /// The system user must belong to a server (and a private thread) to speak in it.
    async fn ensure_access(&self, channel: i64) {
        let row: Option<(Option<i64>, i16)> =
            sqlx::query_as("SELECT guild_id, type FROM channels WHERE id = $1")
                .bind(channel)
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten();
        let Some((Some(guild), kind)) = row else {
            return;
        };
        let _ = sqlx::query(
            "INSERT INTO guild_members (guild_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(guild)
        .bind(SYSTEM_USER_ID)
        .execute(&self.pool)
        .await;
        if matches!(kind, 10..=12) {
            let _ = sqlx::query(
                "INSERT INTO thread_members (channel_id, user_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(channel)
            .bind(SYSTEM_USER_ID)
            .execute(&self.pool)
            .await;
        }
    }

    /// Store `req` as `author` in `channel` and announce it.
    async fn deliver(
        &self,
        author: UserId,
        channel: ChannelId,
        req: SendMessage,
    ) -> Result<MessageId, SendFailure> {
        let acc = self
            .messages
            .access(author, channel)
            .await
            .map_err(|e| SendFailure::Permanent(format!("{e:?}")))?;
        // AutoMod applies to people, not to the platform's own voice.
        if author != system()
            && let (Some(gate), Some(guild)) = (&self.automod, acc.channel.guild_id)
        {
            let check = MessageCheck {
                guild,
                channel,
                author,
                content: req.content.clone(),
                ..MessageCheck::default()
            };
            if let Ok(v) = gate.evaluate(&check).await
                && v.block.is_some()
            {
                return Err(SendFailure::Permanent(
                    "The message was blocked by this server's AutoMod rules.".into(),
                ));
            }
        }
        let plan = self.messages.plan_dm_announce(&acc.channel, author).await;
        let msg = self
            .messages
            .send(author, channel, req)
            .await
            .map_err(|e| chat_failure(&e))?;
        let data = event_payload(&self.messages, &msg)
            .await
            .unwrap_or_else(|_| json!({}));
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
        if !msg.embeds.is_empty() || msg.content.contains("http") {
            self.messages.schedule_unfurl(&msg).await;
        }
        Ok(msg.id)
    }

    async fn guild_owner(&self, guild: i64) -> Option<UserId> {
        let owner: Option<i64> = sqlx::query_scalar("SELECT owner_id FROM guilds WHERE id = $1")
            .bind(guild)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten();
        owner.map(uid)
    }

    async fn set_role(&self, guild: i64, user: i64, role: i64, add: bool) -> bool {
        // Roles configured by a server's managers are handed out as the owner, who outranks every role.
        let Some(owner) = self.guild_owner(guild).await else {
            return false;
        };
        match self
            .guilds
            .assign_role(
                owner,
                GuildId::new(u64::try_from(guild).unwrap_or(0)),
                uid(user),
                rid(role),
                add,
            )
            .await
        {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(guild, user, role, add, error = %e, "could not change a member's role");
                false
            }
        }
    }
}

#[async_trait]
impl Poster for SystemPoster {
    async fn post(&self, channel: i64, card: Card) -> Result<i64, String> {
        self.ensure_access(channel).await;
        let req = card_to_send(&card, &self.base_url);
        match self.deliver(system(), cid(channel), req).await {
            Ok(id) => Ok(raw(id.get())),
            Err(SendFailure::Permanent(m) | SendFailure::Retry(m)) => Err(m),
        }
    }

    async fn dm(&self, user: i64, card: Card) -> Result<(i64, i64), String> {
        let social = PgSocialRepo::new(self.pool.clone());
        let target = uid(user);
        let channel = match social.find_dm(system(), target).await {
            Ok(Some(c)) => c.id,
            Ok(None) => {
                let id = ChannelId::new(self.messages.next_id().get());
                social
                    .insert_dm(id, system(), target)
                    .await
                    .map_err(|e| e.to_string())?;
                id
            }
            Err(e) => return Err(e.to_string()),
        };
        let req = card_to_send(&card, &self.base_url);
        match self.deliver(system(), channel, req).await {
            Ok(id) => Ok((raw(channel.get()), raw(id.get()))),
            Err(SendFailure::Permanent(m) | SendFailure::Retry(m)) => Err(m),
        }
    }

    async fn send_as(&self, author: i64, channel: i64, content: &str) -> Result<i64, SendFailure> {
        let req = SendMessage {
            content: content.to_owned(),
            ..SendMessage::default()
        };
        self.deliver(uid(author), cid(channel), req)
            .await
            .map(|id| raw(id.get()))
    }

    async fn react(&self, channel: i64, message: i64, emoji: &str) {
        let Some(key) = EmojiKey::parse(emoji) else {
            return;
        };
        let (channel, message) = (
            cid(channel),
            MessageId::new(u64::try_from(message).unwrap_or(0)),
        );
        let msg = match self.messages.repo.get_message(channel, message).await {
            Ok(Some(m)) => m,
            _ => return,
        };
        match self
            .messages
            .store
            .add_reaction(channel, message, system(), &key, self.messages.now_ms())
            .await
        {
            Ok(true) => {
                self.messages
                    .publish(DomainEvent::ReactionAdded {
                        guild: msg.guild_id,
                        channel,
                        message_id: message.get(),
                        user: system(),
                        emoji: key.json(),
                    })
                    .await;
            }
            Ok(false) => {}
            Err(e) => tracing::warn!(error = %e, "could not seed a vote reaction"),
        }
    }

    async fn edit(&self, channel: i64, message: i64, card: Card) {
        let (channel, message) = (
            cid(channel),
            MessageId::new(u64::try_from(message).unwrap_or(0)),
        );
        let req = card_to_send(&card, &self.base_url);
        let embeds = serde_json::to_value(&req.embeds).unwrap_or(Value::Null);
        // Embeds are not editable through the message service (people cannot edit embeds), so the system
        // user's own card is rewritten in place.
        let updated = sqlx::query(
            "UPDATE messages SET content = $2, embeds = $3, edited_at = now() \
             WHERE id = $1 AND author_id = $4",
        )
        .bind(raw(message.get()))
        .bind(&req.content)
        .bind(embeds)
        .bind(SYSTEM_USER_ID)
        .execute(&self.pool)
        .await;
        if !matches!(updated, Ok(ref r) if r.rows_affected() == 1) {
            return;
        }
        if let Ok(Some(msg)) = self.messages.repo.get_message(channel, message).await
            && let Ok(data) = event_payload(&self.messages, &msg).await
        {
            self.messages
                .publish(DomainEvent::MessageEdited {
                    guild: msg.guild_id,
                    channel,
                    message_id: msg.id.get(),
                    data,
                })
                .await;
        }
    }

    async fn add_role(&self, guild: i64, user: i64, role: i64) -> bool {
        self.set_role(guild, user, role, true).await
    }

    async fn remove_role(&self, guild: i64, user: i64, role: i64) -> bool {
        self.set_role(guild, user, role, false).await
    }
}

// ------------------------------------------------------------------ the scheduler job

/// Register `userfeat.tick`. Pair with [`schedule`].
pub fn register(runner: JobRunner, pool: &PgPool, ticker: Arc<Ticker>) -> JobRunner {
    let pool = pool.clone();
    runner.register(TICK_KIND, 1, move |job| {
        Box::pin(tick(pool.clone(), ticker.clone(), job))
    })
}

async fn tick(pool: PgPool, ticker: Arc<Ticker>, job: Job) -> Result<(), HandlerError> {
    let report = ticker.run_once(false).await;
    let busy = report.scheduled_sent
        + report.scheduled_failed
        + report.reminders_sent
        + report.reminders_failed
        + report.feed_posts
        + report.library_posts
        + report.birthdays;
    if busy > 0 {
        tracing::info!(?report, "feature modules dispatched");
    }
    jobs::reschedule_running(&pool, job.id, TICK_INTERVAL).await?;
    Ok(())
}

/// Create or re-arm the scheduler singleton. Call only when [`register`] is on the runner.
pub async fn schedule(pool: &PgPool) -> DbResult<()> {
    jobs::schedule_singleton(
        pool,
        TICK_JOB_ID,
        TICK_KIND,
        &json!({}),
        Duration::from_secs(5),
    )
    .await?;
    Ok(())
}

/// The SSRF-safe client the RSS poster fetches with. `allow_private` follows the unfurl policy knobs.
pub fn feed_client(policy: sr_net::Policy) -> Arc<SafeClient> {
    Arc::new(SafeClient::production(policy, "SIDEROOM-Feeds/1.0"))
}

// ------------------------------------------------------------------ leveling

/// Awards XP for each message a person sends in a server that runs leveling.
pub struct LevelingHook {
    pub uf: UserFeatures,
    pub poster: PosterCell,
}

#[async_trait]
impl MessageHooks for LevelingHook {
    async fn message_sent(&self, m: &Message, _notify: &[UserId]) {
        let Some(guild) = m.guild_id else {
            return;
        };
        if !matches!(m.kind, MessageType::Default | MessageType::Reply) || m.author_id == system() {
            return;
        }
        let guild = raw(guild.get());
        // Cached flag check first: servers without leveling pay nothing more.
        if !self
            .uf
            .enabled(LEVELING_ID, Some(guild))
            .await
            .unwrap_or(false)
        {
            return;
        }
        // Never delay the message: the award happens off the send path.
        let (uf, poster) = (self.uf.clone(), self.poster.clone());
        let (author, channel, message) =
            (raw(m.author_id.get()), raw(m.channel_id.get()), m.id.get());
        tokio::spawn(async move {
            if let Err(e) = award(&uf, poster.get().cloned(), guild, author, channel, message).await
            {
                tracing::debug!(error = %e, "leveling award failed");
            }
        });
    }
}

async fn award(
    uf: &UserFeatures,
    poster: Option<Arc<dyn Poster>>,
    guild: i64,
    author: i64,
    channel: i64,
    message: u64,
) -> Result<(), sr_userfeat::UfError> {
    let cfg = uf.settings(LEVELING_ID, Some(guild)).await?;
    if cfg.ids("ignored_channels").contains(&channel) {
        return Ok(());
    }
    let bot: bool =
        sqlx::query_scalar("SELECT COALESCE(is_bot, FALSE) FROM accounts WHERE id = $1")
            .bind(author)
            .fetch_optional(&uf.pool)
            .await?
            .unwrap_or(true);
    if bot {
        return Ok(());
    }
    let xp = pick_xp(cfg.int("xp_min", 15), cfg.int("xp_max", 25), message);
    let cooldown = cfg.int("cooldown", 60);
    let Some(up) = uf.award_xp(guild, author, xp, cooldown).await? else {
        return Ok(());
    };
    let Some(poster) = poster else {
        return Ok(());
    };
    for role in roles_between(&cfg, up.old, up.new) {
        poster.add_role(guild, author, role).await;
    }
    if cfg.flag("announce", true) {
        let target = cfg.id("announce_channel").unwrap_or(channel);
        let card = Card {
            content: format!("<@{author}> reached level {}!", up.new),
            color: 0x9B59B6,
            ping_users: vec![author],
            ..Card::default()
        };
        let _ = poster.post(target, card).await;
    }
    Ok(())
}

// ------------------------------------------------------------------ watch party announcer

/// Tells the announcer module when a watch party starts or ends.
pub struct PartyAnnouncer {
    pub uf: UserFeatures,
    pub poster: PosterCell,
}

#[async_trait]
impl sr_watchparty::ChatHook for PartyAnnouncer {
    async fn announce(&self, a: sr_watchparty::Announcement) {
        let Some(poster) = self.poster.get() else {
            return;
        };
        let party = PartyAnnouncement {
            party_id: a.party_id,
            channel_id: a.channel_id,
            host_id: a.host_id,
            title: a.title,
            episode_label: a.episode_label,
            viewers: a.viewers,
            started: a.kind == sr_watchparty::AnnouncementKind::Started,
        };
        if let Err(e) = self.uf.announce_party(&**poster, &party).await {
            tracing::warn!(error = %e, "watch party announcement failed");
        }
    }
}

//! PRF-SCR-001 Scrapbook: a server's vault of messages worth keeping. A message that collects enough of a
//! trigger reaction, or that a permitted member sends by hand ("Send to Scrapbook" in the message menu), is
//! copied into the server's own bucket and described by an entry; deleting the message deletes the entry and
//! its copies.
//!
//! The module is **hidden until the operator enables it at platform scope** ([`Feature::operator_gated`]):
//! until then it is off everywhere and absent from Guild Settings > Modules.
//!
//! This file is the module's side of the feature registry: the settings a server's managers edit, the
//! reaction trigger, and the typed [`ScrapbookConfig`] the rest of the platform reads. The copy itself needs
//! storage and the database, which modules do not have; the trigger therefore leaves a *request* row in the
//! module's runtime state (`req:<message>`, due at once) and the server's job `scrapbook.requests`
//! (`sr-server`) performs it with `sr_storage::scrapbook::Archiver`. The bucket cascade on message delete
//! is not here for the same reason: it must run whether or not the module is still switched on.
//!
//! * Reactions count distinct users, the author's own reaction included (a scrapbook is not a popularity
//!   contest; `self_star` is a Starboard idea).
//! * Age-restricted channels are skipped unless `nsfw` says `allow`.
//! * `roles` only governs the manual path: members with one of these roles, or `MANAGE_MESSAGES`, see
//!   "Send to Scrapbook". The reaction path needs no role: the threshold is the gate.

use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::{Value, json};
use sr_events::{ChannelId, DomainEvent, GuildId};

use crate::feature::{Feature, FeatureCtx, FeatureError, FeatureMeta, Scope};
use crate::schema::{EnumOption, Field, FieldKind, SettingsSchema};
use crate::state::StateEntry;

use super::{event_emoji_key, normalize_emoji, snowflake, snowflakes};

/// Feature id.
pub const SCRAPBOOK_ID: &str = "lounge.scrapbook";
/// Largest object the copy takes, in MiB, and the platform setting's default (`scrapbook.max_object_mib`).
pub const MAX_OBJECT_MIB: u32 = 200;
/// Prefix of the request rows in the module's state.
pub const REQUEST_PREFIX: &str = "req:";

/// State key of the request for `message`.
pub fn request_key(message: u64) -> String {
    format!("{REQUEST_PREFIX}{message}")
}

/// The module's settings, typed. Missing keys fall back to the defaults below.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ScrapbookConfig {
    /// Trigger emoji: unicode, or `name:id` for a custom one.
    pub emojis: Vec<String>,
    /// Distinct reactors needed.
    pub threshold: u32,
    /// Roles allowed to send a message by hand (besides `MANAGE_MESSAGES`).
    pub roles: Vec<String>,
    /// Channels where the reaction trigger is off.
    pub ignored_channels: Vec<String>,
    /// `skip` (default) or `allow`: messages in age-restricted channels.
    pub nsfw: String,
    /// Largest object copied, MiB. Read at platform scope.
    pub max_object_mib: u32,
}

impl Default for ScrapbookConfig {
    fn default() -> Self {
        Self {
            emojis: vec!["\u{1f4cc}".into()],
            threshold: 5,
            roles: Vec::new(),
            ignored_channels: Vec::new(),
            nsfw: "skip".into(),
            max_object_mib: MAX_OBJECT_MIB,
        }
    }
}

impl ScrapbookConfig {
    /// The normalised trigger emoji keys.
    pub fn emoji_keys(&self) -> Vec<String> {
        self.emojis
            .iter()
            .map(|e| normalize_emoji(e))
            .filter(|e| !e.is_empty())
            .collect()
    }

    /// Whether `key` (an event's emoji key) is a trigger.
    pub fn is_trigger(&self, key: &str) -> bool {
        self.emoji_keys().iter().any(|e| e == key)
    }

    /// Whether the reaction trigger is off in `channel`.
    pub fn channel_ignored(&self, channel: u64) -> bool {
        snowflakes(&self.ignored_channels).contains(&channel)
    }

    /// Whether a member holding `role_ids` may send by hand through a role (not counting `MANAGE_MESSAGES`).
    pub fn role_allowed(&self, role_ids: &[u64]) -> bool {
        let allowed = snowflakes(&self.roles);
        role_ids.iter().any(|r| allowed.contains(r))
    }

    /// The copy cap in bytes, clamped to 1..=[`MAX_OBJECT_MIB`] MiB.
    pub fn max_object_bytes(&self) -> u64 {
        u64::from(self.max_object_mib.clamp(1, MAX_OBJECT_MIB)) * 1024 * 1024
    }

    /// Parse effective settings; a malformed value is the default.
    pub fn from_values(values: &serde_json::Map<String, Value>) -> Self {
        serde_json::from_value(Value::Object(values.clone())).unwrap_or_default()
    }
}

/// The copy cap in bytes for a server: the smaller of the platform's `max_object_mib` and the default
/// ceiling. (Servers do not set it; the field is platform-only.)
pub fn effective_max_bytes(platform: &ScrapbookConfig) -> u64 {
    platform.max_object_bytes()
}

#[derive(Default)]
pub struct Scrapbook;

impl Scrapbook {
    async fn handle(&self, ev: &DomainEvent, ctx: &FeatureCtx) -> Result<(), FeatureError> {
        let DomainEvent::ReactionAdded {
            guild: Some(g),
            channel,
            message_id,
            emoji,
            ..
        } = ev
        else {
            return Ok(());
        };
        let Some(key) = event_emoji_key(emoji) else {
            return Ok(());
        };
        self.consider(ctx, *g, *channel, *message_id, &key).await
    }

    /// A trigger reaction arrived: leave a request when the message now has enough of them.
    async fn consider(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        channel: ChannelId,
        message: u64,
        key: &str,
    ) -> Result<(), FeatureError> {
        let cfg: ScrapbookConfig = ctx.guild_config(SCRAPBOOK_ID, guild).await?;
        if !cfg.is_trigger(key) || cfg.channel_ignored(channel.get()) {
            return Ok(());
        }
        let Some(msg) = ctx.actions.get_message(channel, message).await? else {
            return Ok(());
        };
        // Never archive the platform's own posts (starboard reposts, system notices).
        if msg.author == ctx.actions.self_user() {
            return Ok(());
        }
        let source = ctx.actions.get_channel(channel).await?;
        if source.as_ref().is_some_and(|c| c.nsfw) && cfg.nsfw != "allow" {
            return Ok(());
        }
        let count = ctx
            .actions
            .reaction_count(channel, message, key, None)
            .await?;
        if count < cfg.threshold.max(1) {
            return Ok(());
        }
        let k = request_key(message);
        if ctx.state.get(SCRAPBOOK_ID, guild, &k).await?.is_some() {
            return Ok(());
        }
        ctx.state
            .put(
                StateEntry::new(
                    SCRAPBOOK_ID,
                    guild,
                    k,
                    json!({"channel": channel.get(), "via": "reaction", "attempts": 0}),
                )
                .due(ctx.now_ms()),
            )
            .await?;
        Ok(())
    }
}

impl Feature for Scrapbook {
    fn id(&self) -> &'static str {
        SCRAPBOOK_ID
    }

    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Scrapbook".into(),
            description: "A vault of messages worth keeping. Reactions, or a member with the right role, \
                          copy a message and its files into this server's own storage, where the \
                          server can browse them as a gallery."
                .into(),
            section: "Lounge".into(),
            phase: 34,
            default_enabled: false,
        }
    }

    /// Hidden until the operator enables it at platform scope.
    fn operator_gated(&self) -> bool {
        true
    }

    /// The web shows the Scrapbook entry and the channel banner only when the operator opted in.
    fn client_visible(&self) -> bool {
        true
    }

    fn settings_schema(&self) -> SettingsSchema {
        let opt = |v: &str, l: &str| EnumOption {
            value: v.into(),
            label: l.into(),
        };
        SettingsSchema::new(vec![
            Field::new(
                "emojis",
                "Trigger emoji",
                FieldKind::List {
                    item: Box::new(FieldKind::String { max_len: Some(64) }),
                    max_items: Some(5),
                },
            )
            .help("Unicode emoji, or name:id for a custom one. A message with enough of any of them is archived.")
            .default_value(json!(["\u{1f4cc}"])),
            Field::new(
                "threshold",
                "Reactions needed",
                FieldKind::Int {
                    min: Some(1),
                    max: Some(100),
                },
            )
            .default_value(json!(5)),
            Field::new("roles", "Roles that can send by hand", FieldKind::RolePicker { multi: true })
                .help("Members with one of these roles see \"Send to Scrapbook\" in the message menu. Members who can manage messages always do."),
            Field::new(
                "ignored_channels",
                "Channels where reactions do not archive",
                FieldKind::ChannelPicker { multi: true },
            ),
            Field::new(
                "nsfw",
                "Age-restricted channels",
                FieldKind::Enum {
                    options: vec![
                        opt("skip", "Reactions never archive them"),
                        opt("allow", "Reactions archive them too"),
                    ],
                },
            )
            .default_value(json!("skip")),
            Field::new(
                "max_object_mib",
                "Largest file copied (MiB)",
                FieldKind::Int {
                    min: Some(1),
                    max: Some(i64::from(MAX_OBJECT_MIB)),
                },
            )
            .help("Files larger than this are not copied; the message text still is.")
            .default_value(json!(MAX_OBJECT_MIB))
            .at(&[Scope::Platform]),
        ])
    }

    fn scopes(&self) -> &[Scope] {
        &[Scope::Platform, Scope::Guild]
    }

    fn on_event<'a>(
        &'a self,
        ev: &'a DomainEvent,
        ctx: &'a FeatureCtx,
    ) -> BoxFuture<'a, Result<(), FeatureError>> {
        Box::pin(self.handle(ev, ctx))
    }
}

/// Snowflake helper re-exported for the job that reads requests.
pub fn parse_message_key(key: &str) -> Option<u64> {
    key.strip_prefix(REQUEST_PREFIX).and_then(snowflake)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use sr_events::UserId;

    use super::*;
    use crate::actions::{AttachmentInfo, ChannelInfo, MessageInfo};
    use crate::testing::Harness;
    use crate::{FeatureRegistry, ScopeRef};

    const G: GuildId = GuildId::new(1);
    const SRC: ChannelId = ChannelId::new(10);
    const MSG: u64 = 500;
    const AUTHOR: UserId = UserId::new(7);
    const PIN: &str = "\u{1f4cc}";

    fn channel(id: u64, nsfw: bool) -> ChannelInfo {
        ChannelInfo {
            id: ChannelId::new(id),
            guild: Some(G),
            kind: 0,
            name: format!("c{id}"),
            nsfw,
            parent: None,
            rate_limit_per_user: 0,
            everyone_can_view: true,
        }
    }

    async fn harness(settings: Value) -> Harness {
        let mut reg = FeatureRegistry::new();
        reg.register(Scrapbook).unwrap();
        let h = Harness::new(reg).await;
        h.configure(SCRAPBOOK_ID, settings).await;
        h.actions.put_user(7, "Ada");
        h.actions.put_channel(channel(10, false));
        h.actions.put_channel(channel(11, true));
        h.actions.put_message(MessageInfo {
            id: MSG,
            channel: SRC,
            guild: Some(G),
            author: AUTHOR,
            content: "frame this".into(),
            embeds: vec![],
            attachments: vec![AttachmentInfo {
                url: "https://x/y.png".into(),
                filename: "y.png".into(),
                content_type: Some("image/png".into()),
            }],
        });
        h
    }

    fn added(channel: ChannelId, user: u64, name: &str) -> DomainEvent {
        DomainEvent::ReactionAdded {
            guild: Some(G),
            channel,
            message_id: MSG,
            user: UserId::new(user),
            emoji: json!({"id": null, "name": name}),
        }
    }

    async fn fire(h: &Harness, ev: DomainEvent) {
        Scrapbook.on_event(&ev, &h.ctx).await.unwrap();
    }

    async fn requests(h: &Harness) -> Vec<String> {
        h.ctx
            .state
            .list(SCRAPBOOK_ID, G, REQUEST_PREFIX)
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.key)
            .collect()
    }

    #[tokio::test]
    async fn a_request_appears_at_the_threshold_and_only_then() {
        let h = harness(json!({"threshold": 2})).await;
        h.actions.react(MSG, PIN, 1);
        fire(&h, added(SRC, 1, PIN)).await;
        assert!(requests(&h).await.is_empty(), "one reaction is below two");
        h.actions.react(MSG, PIN, 2);
        fire(&h, added(SRC, 2, PIN)).await;
        assert_eq!(requests(&h).await, vec![request_key(MSG)]);
        let due = h.ctx.state.due(SCRAPBOOK_ID, h.now(), 10).await.unwrap();
        assert_eq!(due.len(), 1, "the request is due at once");
        // A third reaction does not queue a second request.
        h.actions.react(MSG, PIN, 3);
        fire(&h, added(SRC, 3, PIN)).await;
        assert_eq!(requests(&h).await.len(), 1);
    }

    #[tokio::test]
    async fn the_authors_own_reaction_counts() {
        let h = harness(json!({"threshold": 2})).await;
        h.actions.react(MSG, PIN, AUTHOR.get());
        h.actions.react(MSG, PIN, 2);
        fire(&h, added(SRC, 2, PIN)).await;
        assert_eq!(requests(&h).await.len(), 1);
    }

    #[tokio::test]
    async fn other_emoji_and_ignored_channels_do_nothing() {
        let h = harness(json!({"threshold": 1, "ignored_channels": ["10"]})).await;
        h.actions.react(MSG, "\u{2b50}", 1);
        fire(&h, added(SRC, 1, "\u{2b50}")).await;
        h.actions.react(MSG, PIN, 1);
        fire(&h, added(SRC, 1, PIN)).await;
        assert!(requests(&h).await.is_empty());
    }

    #[tokio::test]
    async fn age_restricted_channels_are_skipped_unless_allowed() {
        let h = harness(json!({"threshold": 1})).await;
        h.actions.put_message(MessageInfo {
            id: MSG,
            channel: ChannelId::new(11),
            guild: Some(G),
            author: AUTHOR,
            content: "spicy".into(),
            embeds: vec![],
            attachments: vec![],
        });
        h.actions.react(MSG, PIN, 1);
        fire(&h, added(ChannelId::new(11), 1, PIN)).await;
        assert!(requests(&h).await.is_empty());
        h.configure(SCRAPBOOK_ID, json!({"threshold": 1, "nsfw": "allow"}))
            .await;
        fire(&h, added(ChannelId::new(11), 1, PIN)).await;
        assert_eq!(requests(&h).await.len(), 1);
    }

    #[tokio::test]
    async fn the_platforms_own_posts_are_never_archived() {
        let h = harness(json!({"threshold": 1})).await;
        h.actions.put_message(MessageInfo {
            id: MSG,
            channel: SRC,
            guild: Some(G),
            author: h.ctx.actions.self_user(),
            content: "system".into(),
            embeds: vec![],
            attachments: vec![],
        });
        h.actions.react(MSG, PIN, 1);
        fire(&h, added(SRC, 1, PIN)).await;
        assert!(requests(&h).await.is_empty());
    }

    #[test]
    fn config_roles_and_cap() {
        let cfg = ScrapbookConfig {
            roles: vec!["5".into(), "x".into()],
            max_object_mib: 9999,
            ..ScrapbookConfig::default()
        };
        assert!(cfg.role_allowed(&[1, 5]));
        assert!(!cfg.role_allowed(&[1, 2]));
        assert!(!ScrapbookConfig::default().role_allowed(&[1]));
        assert_eq!(cfg.max_object_bytes(), 200 * 1024 * 1024);
        assert_eq!(
            ScrapbookConfig::from_values(&serde_json::Map::new()),
            ScrapbookConfig::default()
        );
        assert_eq!(parse_message_key("req:42"), Some(42));
        assert_eq!(parse_message_key("link:42"), None);
    }

    #[test]
    fn schema_defaults_validate_and_the_cap_is_platform_only() {
        let s = Scrapbook.settings_schema();
        s.validate(&s.defaults(Scope::Platform)).unwrap();
        s.validate(&s.defaults(Scope::Guild)).unwrap();
        assert!(
            s.for_scope(Scope::Guild)
                .fields
                .iter()
                .all(|f| f.key != "max_object_mib")
        );
        assert!(
            s.for_scope(Scope::Platform)
                .fields
                .iter()
                .any(|f| f.key == "max_object_mib")
        );
        assert!(!Scrapbook.meta().default_enabled);
        assert!(Scrapbook.operator_gated());
    }

    /// Hidden by default: a server cannot switch it on until the operator opted in at platform scope, and the
    /// kill switch still wins afterwards.
    #[tokio::test]
    async fn hidden_until_the_operator_enables_it_at_platform_scope() {
        let mut reg = FeatureRegistry::new();
        reg.register(Scrapbook).unwrap();
        let svc = crate::FeatureService::new(
            std::sync::Arc::new(reg),
            std::sync::Arc::new(crate::InMemorySettingsStore::new()),
            None,
        );
        let guild = ScopeRef::Guild { guild: G };
        assert!(!svc.offered(SCRAPBOOK_ID).await.unwrap());
        assert!(!svc.is_enabled(SCRAPBOOK_ID, &guild).await.unwrap());
        // A server's own switch is not enough while the operator has not opted in.
        svc.set_enabled(SCRAPBOOK_ID, &guild, Some(true))
            .await
            .unwrap();
        assert!(!svc.is_enabled(SCRAPBOOK_ID, &guild).await.unwrap());
        // The operator opts in: the server's switch now counts.
        svc.set_enabled(SCRAPBOOK_ID, &ScopeRef::Platform, Some(true))
            .await
            .unwrap();
        assert!(svc.offered(SCRAPBOOK_ID).await.unwrap());
        assert!(svc.is_enabled(SCRAPBOOK_ID, &guild).await.unwrap());
        // Another server that did not switch it on stays off (default_enabled is false).
        let other = ScopeRef::Guild {
            guild: GuildId::new(2),
        };
        assert!(!svc.is_enabled(SCRAPBOOK_ID, &other).await.unwrap());
        // The kill switch ends it everywhere, and hides it again.
        svc.set_enabled(SCRAPBOOK_ID, &ScopeRef::Platform, Some(false))
            .await
            .unwrap();
        assert!(!svc.is_enabled(SCRAPBOOK_ID, &guild).await.unwrap());
        assert!(!svc.offered(SCRAPBOOK_ID).await.unwrap());
        // Clearing the platform flag goes back to hidden.
        svc.set_enabled(SCRAPBOOK_ID, &ScopeRef::Platform, None)
            .await
            .unwrap();
        assert!(!svc.is_enabled(SCRAPBOOK_ID, &guild).await.unwrap());
    }
}

//! FTR-JTC-001 Join-to-create voice: joining a hub voice channel makes a private room for
//! the member and moves them into it; the room is deleted when the last member leaves.
//!
//! * **Hub channels** are voice channels an admin names; joining one is the trigger. The
//!   hub itself stays empty.
//! * **Name template** with `{user}` (the member's display name) and `{server}`.
//! * **Owner controls**: the room's creator may manage that one channel, so rename, user
//!   limit and lock (denying `Connect` to `@everyone` in the channel editor) are the normal
//!   channel settings, and they can move members within the room. Switch this off to keep
//!   rooms admin-managed.
//! * One room per member: joining a hub while already owning a room moves them back into
//!   it instead of making another. A server keeps at most [`MAX_ROOMS`] rooms.
//!
//! Rooms are tracked in feature state (`room:<channel>` and `owner:<user>`); a tick sweeps
//! rooms that are empty or whose channel vanished, so a crash between creating a room and
//! moving the member in never leaves clutter behind.

use futures::future::BoxFuture;
use serde::Deserialize;
use serde_json::json;
use sr_events::{ChannelId, DomainEvent, GuildId, UserId};

use crate::feature::{Feature, FeatureCtx, FeatureError, FeatureMeta, Scope};
use crate::schema::{Field, FieldKind, SettingsSchema};
use crate::state::StateEntry;
use crate::template::{clip, render};

use super::{snowflake, snowflakes};

pub const JOIN_TO_CREATE_ID: &str = "lounge.join_to_create";

/// Most rooms one server may have open through this module.
pub const MAX_ROOMS: usize = 50;
/// A fresh room is not swept for this long, giving the move time to land.
const GRACE_MS: i64 = 20_000;

#[derive(Default)]
pub struct JoinToCreate;

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Config {
    hubs: Vec<String>,
    name_template: String,
    category: Option<String>,
    user_limit: u32,
    owner_controls: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hubs: Vec::new(),
            name_template: "{user}'s room".into(),
            category: None,
            user_limit: 0,
            owner_controls: true,
        }
    }
}

fn room_key(channel: u64) -> String {
    format!("room:{channel}")
}

fn owner_key(user: u64) -> String {
    format!("owner:{user}")
}

/// A channel name from the template: no control characters, at most 100 characters, never
/// empty.
fn room_name(template: &str, user: &str, server: &str) -> String {
    let rendered = render(
        template,
        &[("user", user.to_owned()), ("server", server.to_owned())],
    );
    let clean: String = rendered.chars().filter(|c| !c.is_control()).collect();
    let name = clip(clean.trim(), 100);
    if name.is_empty() {
        "Room".to_owned()
    } else {
        name
    }
}

impl JoinToCreate {
    /// A member entered `hub`: give them a room (or take them back to the one they have).
    async fn hub_joined(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        hub: ChannelId,
        user: UserId,
        cfg: &Config,
    ) -> Result<(), FeatureError> {
        // Still the owner of a live room? Go back to it.
        if let Some(entry) = ctx
            .state
            .get(JOIN_TO_CREATE_ID, guild, &owner_key(user.get()))
            .await?
        {
            let existing = entry.value.get("channel").and_then(|v| v.as_u64());
            if let Some(ch) = existing
                && ctx.actions.get_channel(ChannelId::new(ch)).await?.is_some()
            {
                ctx.actions
                    .move_member(guild, user, ChannelId::new(ch))
                    .await?;
                return Ok(());
            }
            ctx.state
                .delete(JOIN_TO_CREATE_ID, guild, &owner_key(user.get()))
                .await?;
        }
        let open = ctx.state.list(JOIN_TO_CREATE_ID, guild, "room:").await?.len();
        if open >= MAX_ROOMS {
            tracing::warn!(guild = guild.get(), "join-to-create: too many open rooms");
            return Ok(());
        }
        let user_name = ctx
            .actions
            .get_user(user)
            .await?
            .map_or_else(|| "Member".to_owned(), |u| u.name);
        let server = ctx
            .actions
            .get_guild(guild)
            .await?
            .map_or_else(String::new, |g| g.name);
        let parent = match cfg.category.as_deref().and_then(snowflake) {
            Some(c) => Some(ChannelId::new(c)),
            None => ctx.actions.get_channel(hub).await?.and_then(|c| c.parent),
        };
        let name = room_name(&cfg.name_template, &user_name, &server);
        let room = ctx
            .actions
            .create_voice_channel(
                guild,
                &name,
                parent,
                cfg.user_limit.min(99),
                cfg.owner_controls.then_some(user),
            )
            .await?;
        let now = ctx.now_ms();
        ctx.state
            .put(StateEntry::new(
                JOIN_TO_CREATE_ID,
                guild,
                room_key(room.get()),
                json!({"owner": user.get(), "hub": hub.get(), "at_ms": now}),
            ))
            .await?;
        ctx.state
            .put(StateEntry::new(
                JOIN_TO_CREATE_ID,
                guild,
                owner_key(user.get()),
                json!({"channel": room.get()}),
            ))
            .await?;
        if let Err(e) = ctx.actions.move_member(guild, user, room).await {
            // The member left before the move: do not keep an empty room around.
            self.remove_room(ctx, guild, room).await?;
            tracing::debug!(error = %e, "join-to-create: the member left before the move");
        }
        Ok(())
    }

    /// Delete a tracked room and forget it.
    async fn remove_room(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        room: ChannelId,
    ) -> Result<(), FeatureError> {
        if let Some(entry) = ctx
            .state
            .get(JOIN_TO_CREATE_ID, guild, &room_key(room.get()))
            .await?
        {
            if let Some(owner) = entry.value.get("owner").and_then(|v| v.as_u64()) {
                ctx.state
                    .delete(JOIN_TO_CREATE_ID, guild, &owner_key(owner))
                    .await?;
            }
            ctx.state
                .delete(JOIN_TO_CREATE_ID, guild, &room_key(room.get()))
                .await?;
        }
        match ctx.actions.delete_channel(room).await {
            Ok(()) | Err(crate::actions::ActionError::NotFound) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Delete `room` when nobody is in it.
    async fn sweep_one(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        room: ChannelId,
        fresh_until_ms: Option<i64>,
    ) -> Result<(), FeatureError> {
        if fresh_until_ms.is_some_and(|t| ctx.now_ms() < t) {
            return Ok(());
        }
        if ctx.actions.get_channel(room).await?.is_none() {
            return self.forget_room(ctx, guild, room).await;
        }
        if ctx.actions.voice_occupants(room).await?.is_empty() {
            self.remove_room(ctx, guild, room).await?;
        }
        Ok(())
    }

    /// The channel is gone already: drop the bookkeeping only.
    async fn forget_room(
        &self,
        ctx: &FeatureCtx,
        guild: GuildId,
        room: ChannelId,
    ) -> Result<(), FeatureError> {
        if let Some(entry) = ctx
            .state
            .get(JOIN_TO_CREATE_ID, guild, &room_key(room.get()))
            .await?
        {
            if let Some(owner) = entry.value.get("owner").and_then(|v| v.as_u64()) {
                ctx.state
                    .delete(JOIN_TO_CREATE_ID, guild, &owner_key(owner))
                    .await?;
            }
            ctx.state
                .delete(JOIN_TO_CREATE_ID, guild, &room_key(room.get()))
                .await?;
        }
        Ok(())
    }
}

impl Feature for JoinToCreate {
    fn id(&self) -> &'static str {
        JOIN_TO_CREATE_ID
    }

    fn meta(&self) -> FeatureMeta {
        FeatureMeta {
            name: "Join to create".into(),
            description: "Joining a hub voice channel makes a private room for the member and \
                          removes it when it empties."
                .into(),
            section: "Lounge".into(),
            phase: 12,
            default_enabled: false,
        }
    }

    fn settings_schema(&self) -> SettingsSchema {
        SettingsSchema::new(vec![
            Field::new(
                "hubs",
                "Hub channels",
                FieldKind::ChannelPicker { multi: true },
            )
            .help("Voice channels that make a room for whoever joins. Nothing happens until one is set."),
            Field::new(
                "name_template",
                "Room name",
                FieldKind::String { max_len: Some(100) },
            )
            .help("Placeholders: {user} {server}")
            .default_value(json!("{user}'s room")),
            Field::new(
                "category",
                "Category for new rooms",
                FieldKind::ChannelPicker { multi: false },
            )
            .help("Leave empty to put rooms in the hub's own category."),
            Field::new(
                "user_limit",
                "User limit",
                FieldKind::Int {
                    min: Some(0),
                    max: Some(99),
                },
            )
            .help("0 means no limit. The room's owner can change it.")
            .default_value(json!(0)),
            Field::new("owner_controls", "Owner controls", FieldKind::Bool)
                .help("Let the member who made a room rename it, set its limit and lock it.")
                .default_value(json!(true)),
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
        Box::pin(async move {
            match ev {
                DomainEvent::VoiceJoined {
                    guild,
                    channel,
                    user,
                } => {
                    if *user == ctx.actions.self_user() {
                        return Ok(());
                    }
                    let cfg: Config = ctx.guild_config(JOIN_TO_CREATE_ID, *guild).await?;
                    if snowflakes(&cfg.hubs).contains(&channel.get()) {
                        self.hub_joined(ctx, *guild, *channel, *user, &cfg).await?;
                    }
                    Ok(())
                }
                DomainEvent::VoiceLeft { guild, channel, .. } => {
                    let tracked = ctx
                        .state
                        .get(JOIN_TO_CREATE_ID, *guild, &room_key(channel.get()))
                        .await?
                        .is_some();
                    if tracked {
                        self.sweep_one(ctx, *guild, *channel, None).await?;
                    }
                    Ok(())
                }
                DomainEvent::ChannelDeleted { guild, channel, .. } => {
                    self.forget_room(ctx, *guild, *channel).await
                }
                _ => Ok(()),
            }
        })
    }

    fn on_tick<'a>(&'a self, ctx: &'a FeatureCtx) -> BoxFuture<'a, Result<(), FeatureError>> {
        Box::pin(async move {
            for guild in ctx.settings.enabled_guilds(JOIN_TO_CREATE_ID).await? {
                let rooms = ctx.state.list(JOIN_TO_CREATE_ID, guild, "room:").await?;
                for r in rooms {
                    let Some(ch) = r.key.strip_prefix("room:").and_then(|c| c.parse::<u64>().ok())
                    else {
                        continue;
                    };
                    let at = r.value.get("at_ms").and_then(|v| v.as_i64()).unwrap_or(0);
                    if let Err(e) = self
                        .sweep_one(ctx, guild, ChannelId::new(ch), Some(at + GRACE_MS))
                        .await
                    {
                        tracing::warn!(guild = guild.get(), error = %e, "join-to-create sweep failed");
                    }
                }
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FeatureRegistry;
    use crate::actions::{ChannelInfo, FeatureActions};
    use crate::testing::{Call, Harness};

    const G: GuildId = GuildId::new(1);
    const HUB: ChannelId = ChannelId::new(10);
    const LOUNGE: ChannelId = ChannelId::new(11);
    const CATEGORY: ChannelId = ChannelId::new(5);
    const ALICE: UserId = UserId::new(100);
    const BOB: UserId = UserId::new(101);

    fn voice(id: ChannelId, parent: Option<ChannelId>) -> ChannelInfo {
        ChannelInfo {
            id,
            guild: Some(G),
            kind: 2,
            name: format!("c{}", id.get()),
            nsfw: false,
            parent,
            rate_limit_per_user: 0,
            everyone_can_view: true,
        }
    }

    async fn harness() -> Harness {
        let mut reg = FeatureRegistry::new();
        reg.register(JoinToCreate).unwrap();
        let h = Harness::new(reg).await;
        h.actions.put_guild(1, "Club", 10);
        h.actions.put_user(100, "Alice");
        h.actions.put_user(101, "Bob");
        h.actions.put_channel(voice(HUB, Some(CATEGORY)));
        h.actions.put_channel(voice(LOUNGE, None));
        h.configure(JOIN_TO_CREATE_ID, json!({"hubs": ["10"]})).await;
        h
    }

    fn joined(channel: ChannelId, user: UserId) -> DomainEvent {
        DomainEvent::VoiceJoined {
            guild: G,
            channel,
            user,
        }
    }

    fn left(channel: ChannelId, user: UserId) -> DomainEvent {
        DomainEvent::VoiceLeft {
            guild: G,
            channel,
            user,
        }
    }

    fn created(h: &Harness) -> Vec<(String, Option<ChannelId>, u32, Option<UserId>)> {
        h.actions
            .calls()
            .into_iter()
            .filter_map(|c| match c {
                Call::CreateVoice(_, name, parent, limit, owner) => {
                    Some((name, parent, limit, owner))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn schema_defaults_validate() {
        let s = JoinToCreate.settings_schema();
        s.validate(&s.defaults(Scope::Guild)).unwrap();
    }

    #[test]
    fn names_are_clean_and_never_empty() {
        assert_eq!(room_name("{user}'s room", "Alice", "Club"), "Alice's room");
        assert_eq!(room_name("{server} / {user}", "A\nB", "Club"), "Club / AB");
        assert_eq!(room_name("   ", "Alice", "Club"), "Room");
        assert_eq!(room_name("{user}", &"x".repeat(300), "c").chars().count(), 100);
    }

    #[tokio::test]
    async fn does_nothing_until_a_hub_is_set_or_for_other_channels() {
        let mut reg = FeatureRegistry::new();
        reg.register(JoinToCreate).unwrap();
        let h = Harness::new(reg).await;
        JoinToCreate
            .on_event(&joined(HUB, ALICE), &h.ctx)
            .await
            .unwrap();
        assert!(h.actions.calls().is_empty(), "no hub configured");
        let h = harness().await;
        JoinToCreate
            .on_event(&joined(LOUNGE, ALICE), &h.ctx)
            .await
            .unwrap();
        assert!(h.actions.calls().is_empty(), "not a hub");
        JoinToCreate
            .on_event(&joined(HUB, UserId::new(1)), &h.ctx)
            .await
            .unwrap();
        assert!(h.actions.calls().is_empty(), "the system user is ignored");
    }

    #[tokio::test]
    async fn joining_a_hub_makes_a_room_and_moves_the_member_in() {
        let h = harness().await;
        h.actions.set_occupants(10, &[100]);
        JoinToCreate
            .on_event(&joined(HUB, ALICE), &h.ctx)
            .await
            .unwrap();
        let made = created(&h);
        assert_eq!(
            made,
            vec![("Alice's room".to_owned(), Some(CATEGORY), 0, Some(ALICE))],
            "named from the template, in the hub's category, owned by the member"
        );
        let room = h
            .actions
            .calls()
            .into_iter()
            .find_map(|c| match c {
                Call::Move(_, u, to) if u == ALICE => Some(to),
                _ => None,
            })
            .expect("moved");
        assert_eq!(h.actions.occupants_of(room.get()), vec![100]);
        assert!(h.actions.occupants_of(10).is_empty());

        // Joining the hub again while the room lives goes back to it, no second room.
        h.actions.set_occupants(10, &[100]);
        JoinToCreate
            .on_event(&joined(HUB, ALICE), &h.ctx)
            .await
            .unwrap();
        assert_eq!(created(&h).len(), 1);
        assert_eq!(h.actions.occupants_of(room.get()), vec![100]);
    }

    #[tokio::test]
    async fn settings_shape_the_room() {
        let h = harness().await;
        h.configure(
            JOIN_TO_CREATE_ID,
            json!({"hubs": ["10"], "name_template": "{server}: {user}", "category": "7",
                   "user_limit": 4, "owner_controls": false}),
        )
        .await;
        h.actions.set_occupants(10, &[101]);
        JoinToCreate
            .on_event(&joined(HUB, BOB), &h.ctx)
            .await
            .unwrap();
        assert_eq!(
            created(&h),
            vec![("Club: Bob".to_owned(), Some(ChannelId::new(7)), 4, None)]
        );
    }

    #[tokio::test]
    async fn the_room_goes_when_the_last_member_leaves() {
        let h = harness().await;
        h.actions.set_occupants(10, &[100]);
        JoinToCreate
            .on_event(&joined(HUB, ALICE), &h.ctx)
            .await
            .unwrap();
        let room = h
            .actions
            .calls()
            .into_iter()
            .find_map(|c| match c {
                Call::Move(_, _, to) => Some(to),
                _ => None,
            })
            .unwrap();
        // Bob joins the room; Alice leaves: the room stays.
        h.actions.set_occupants(room.get(), &[100, 101]);
        h.actions.set_occupants(room.get(), &[101]);
        JoinToCreate
            .on_event(&left(room, ALICE), &h.ctx)
            .await
            .unwrap();
        assert!(h.actions.channel_exists(room.get()));
        // Bob leaves: it goes, and Alice gets a fresh one next time.
        h.actions.set_occupants(room.get(), &[]);
        JoinToCreate
            .on_event(&left(room, BOB), &h.ctx)
            .await
            .unwrap();
        assert!(!h.actions.channel_exists(room.get()));
        h.actions.set_occupants(10, &[100]);
        JoinToCreate
            .on_event(&joined(HUB, ALICE), &h.ctx)
            .await
            .unwrap();
        assert_eq!(created(&h).len(), 2);
        // Leaving a channel the module did not make never deletes it.
        JoinToCreate
            .on_event(&left(LOUNGE, ALICE), &h.ctx)
            .await
            .unwrap();
        assert!(h.actions.channel_exists(LOUNGE.get()));
        assert!(h.actions.channel_exists(HUB.get()));
    }

    #[tokio::test]
    async fn a_member_who_left_before_the_move_leaves_no_room_behind() {
        let h = harness().await;
        // Not connected anywhere: the move fails.
        JoinToCreate
            .on_event(&joined(HUB, ALICE), &h.ctx)
            .await
            .unwrap();
        let rooms: Vec<_> = h
            .actions
            .calls()
            .into_iter()
            .filter(|c| matches!(c, Call::DeleteChannel(_)))
            .collect();
        assert_eq!(rooms.len(), 1, "the empty room was removed again");
        // And the bookkeeping is gone: a retry makes a new one.
        h.actions.set_occupants(10, &[100]);
        JoinToCreate
            .on_event(&joined(HUB, ALICE), &h.ctx)
            .await
            .unwrap();
        assert_eq!(created(&h).len(), 2);
    }

    #[tokio::test]
    async fn a_server_keeps_at_most_fifty_rooms() {
        let h = harness().await;
        for n in 0..MAX_ROOMS as u64 {
            h.state_room(900 + n).await;
        }
        h.actions.set_occupants(10, &[100]);
        JoinToCreate
            .on_event(&joined(HUB, ALICE), &h.ctx)
            .await
            .unwrap();
        assert!(created(&h).is_empty());
    }

    #[tokio::test]
    async fn the_tick_sweeps_empty_rooms_and_forgets_deleted_ones() {
        let h = harness().await;
        h.actions.set_occupants(10, &[100]);
        JoinToCreate
            .on_event(&joined(HUB, ALICE), &h.ctx)
            .await
            .unwrap();
        let room = h
            .actions
            .calls()
            .into_iter()
            .find_map(|c| match c {
                Call::Move(_, _, to) => Some(to),
                _ => None,
            })
            .unwrap();
        // Occupied rooms stay; a fresh empty one is given its grace period.
        JoinToCreate.on_tick(&h.ctx).await.unwrap();
        assert!(h.actions.channel_exists(room.get()));
        h.actions.set_occupants(room.get(), &[]);
        JoinToCreate.on_tick(&h.ctx).await.unwrap();
        assert!(h.actions.channel_exists(room.get()), "still fresh");
        h.clock.advance_ms(GRACE_MS + 1);
        JoinToCreate.on_tick(&h.ctx).await.unwrap();
        assert!(!h.actions.channel_exists(room.get()), "swept once empty");
        // A room deleted by hand is just forgotten.
        h.actions.set_occupants(10, &[100]);
        JoinToCreate
            .on_event(&joined(HUB, ALICE), &h.ctx)
            .await
            .unwrap();
        let again = h
            .actions
            .calls()
            .into_iter()
            .rev()
            .find_map(|c| match c {
                Call::Move(_, _, to) => Some(to),
                _ => None,
            })
            .unwrap();
        h.actions.delete_channel(again).await.unwrap();
        JoinToCreate
            .on_event(
                &DomainEvent::ChannelDeleted {
                    guild: G,
                    channel: again,
                    payload: json!({}),
                    viewers: vec![],
                },
                &h.ctx,
            )
            .await
            .unwrap();
        h.actions.set_occupants(10, &[100]);
        JoinToCreate
            .on_event(&joined(HUB, ALICE), &h.ctx)
            .await
            .unwrap();
        assert_eq!(created(&h).len(), 3, "a new room, not the dead one");
    }

    impl Harness {
        async fn state_room(&self, channel: u64) {
            self.ctx
                .state
                .put(StateEntry::new(
                    JOIN_TO_CREATE_ID,
                    G,
                    room_key(channel),
                    json!({"owner": channel, "hub": 10, "at_ms": 0}),
                ))
                .await
                .unwrap();
        }
    }
}

//! BRG-SYN-003: the structural Discord calls against the stateful fake guild (no Postgres).

#[path = "support/mod.rs"]
mod support;

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use sr_bridge::DiscordApi;
use sr_bridge::discord::CallError;
use sr_bridge::discord_structure::*;
use support::fake_guild::{BASE, FakeGuild};

const TOKEN: &str = "bot-token-xyz";
const GUILD: i64 = 5000;

async fn setup() -> (Arc<FakeGuild>, DiscordApi) {
    let fake = Arc::new(FakeGuild::new(GUILD, 777, TOKEN));
    let api = DiscordApi::with_transport(BASE, fake.clone());
    api.set_struct_writes_per_sec(1000.0).await;
    (fake, api)
}

fn text(name: &str) -> ChannelParams {
    ChannelParams {
        kind: Some(channel_type::TEXT),
        name: Some(name.into()),
        ..ChannelParams::default()
    }
}

#[tokio::test]
async fn role_writes_carry_the_reason_and_echo_into_the_audit_log() {
    let (fake, api) = setup().await;
    let r = api
        .create_role(
            TOKEN,
            GUILD,
            &RoleParams {
                name: Some("Mods".into()),
                permissions: Some(1 << 13),
                ..RoleParams::default()
            },
            &reason_for_op(11),
        )
        .await
        .unwrap();
    assert_eq!(r.name, "Mods");
    assert_eq!(r.position, 1);
    api.edit_role(
        TOKEN,
        GUILD,
        r.id,
        &RoleParams {
            color: Some(0xff0000),
            ..RoleParams::default()
        },
        &reason_for_op(12),
    )
    .await
    .unwrap();
    api.delete_role(TOKEN, GUILD, r.id, &reason_for_op(13))
        .await
        .unwrap();
    let audit = fake.audit();
    assert_eq!(audit.len(), 3);
    assert!(audit.iter().all(|a| is_bridge_reason(a.reason.as_deref())));
    assert_eq!(audit[2].reason.as_deref(), Some("sr-bridge:13"));
    let ev: Vec<String> = fake.drain_events().into_iter().map(|e| e.0).collect();
    assert!(ev.contains(&"GUILD_ROLE_CREATE".to_owned()));
    assert!(ev.contains(&"GUILD_ROLE_DELETE".to_owned()));
    // A Discord user's edit has no bridge reason.
    fake.discord_admin(
        42,
        sr_sdk::http::Method::Post,
        &format!("/guilds/{GUILD}/roles"),
        json!({"name": "ByHand"}),
    );
    assert!(!is_bridge_reason(
        fake.audit().last().unwrap().reason.as_deref()
    ));
}

#[tokio::test]
async fn a_bot_cannot_touch_roles_at_or_above_its_own() {
    let (fake, api) = setup().await;
    let above = fake.seed_role("Above", 0, 5, false);
    let e = api
        .edit_role(TOKEN, GUILD, above, &RoleParams::default(), "r")
        .await
        .unwrap_err();
    assert!(matches!(e, CallError::Status { status: 403, .. }));
    let e = api
        .set_role_positions(TOKEN, GUILD, &[(above, 2)], "r")
        .await
        .unwrap_err();
    assert!(matches!(e, CallError::Status { status: 403, .. }));
    let access = api.bot_access(TOKEN, GUILD, 777).await.unwrap();
    assert!(access.administrator);
    assert!(!access.is_top_role());
}

#[tokio::test]
async fn overwrites_round_trip_and_keep_high_bits() {
    let (_fake, api) = setup().await;
    let cat = api
        .create_channel(
            TOKEN,
            GUILD,
            &ChannelParams {
                kind: Some(channel_type::CATEGORY),
                name: Some("Cat".into()),
                ..ChannelParams::default()
            },
            "r",
        )
        .await
        .unwrap();
    let discord_only = 1u64 << 47;
    let ch = api
        .create_channel(
            TOKEN,
            GUILD,
            &ChannelParams {
                parent_id: Some(Some(cat.id)),
                overwrites: Some(vec![Overwrite::role(GUILD, 0, (1 << 10) | discord_only)]),
                ..text("general")
            },
            "r",
        )
        .await
        .unwrap();
    assert_eq!(ch.parent_id, Some(cat.id));
    assert_eq!(ch.overwrites[0].deny & discord_only, discord_only);
    let ch = api
        .set_overwrites(TOKEN, ch.id, &[Overwrite::role(9, 1 << 11, 0)], "r")
        .await
        .unwrap();
    assert_eq!(ch.overwrites.len(), 1);
    api.put_overwrite(TOKEN, ch.id, Overwrite::role(10, 1, 2), "r")
        .await
        .unwrap();
    api.delete_overwrite(TOKEN, ch.id, 9, "r").await.unwrap();
    let got = api.get_channel(TOKEN, ch.id).await.unwrap();
    assert_eq!(got.overwrites, vec![Overwrite::role(10, 1, 2)]);
}

#[tokio::test]
async fn community_only_types_are_refused_until_community() {
    let (fake, api) = setup().await;
    let forum = ChannelParams {
        kind: Some(channel_type::FORUM),
        name: Some("ideas".into()),
        available_tags: Some(vec![ForumTag {
            id: None,
            name: "bug".into(),
            moderated: false,
            emoji_id: None,
            emoji_name: None,
        }]),
        ..ChannelParams::default()
    };
    let e = api
        .create_channel(TOKEN, GUILD, &forum, "r")
        .await
        .unwrap_err();
    assert!(matches!(e, CallError::Status { status: 400, .. }));
    assert!(!api.guild_info(TOKEN, GUILD).await.unwrap().is_community());
    fake.set_community(true);
    let f = api.create_channel(TOKEN, GUILD, &forum, "r").await.unwrap();
    assert!(f.available_tags[0].id.is_some());
}

#[tokio::test]
async fn threads_and_forum_posts_through_the_webhook() {
    let (fake, api) = setup().await;
    fake.set_community(true);
    let forum = api
        .create_channel(
            TOKEN,
            GUILD,
            &ChannelParams {
                kind: Some(channel_type::FORUM),
                name: Some("f".into()),
                ..ChannelParams::default()
            },
            "r",
        )
        .await
        .unwrap();
    let wh = api.create_webhook(TOKEN, forum.id, "bridge").await.unwrap();
    let post = sr_bridge::discord::WebhookPost {
        content: "first".into(),
        username: "ann".into(),
        avatar_url: None,
        ping_users: vec![],
    };
    let posted = api
        .execute_webhook_in(
            wh.id,
            &wh.token,
            &post,
            &ThreadTarget {
                thread_id: None,
                thread_name: Some("A post".into()),
                applied_tags: vec![123],
            },
        )
        .await
        .unwrap();
    let thread = api.get_channel(TOKEN, posted.channel_id).await.unwrap();
    assert_eq!(thread.parent_id, Some(forum.id));
    assert_eq!(thread.applied_tags, vec![123]);
    let again = api
        .execute_webhook_in(
            wh.id,
            &wh.token,
            &post,
            &ThreadTarget {
                thread_id: Some(thread.id),
                ..ThreadTarget::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(again.channel_id, thread.id);
    api.edit_channel(
        TOKEN,
        thread.id,
        &ChannelParams {
            archived: Some(true),
            locked: Some(true),
            ..ChannelParams::default()
        },
        "r",
    )
    .await
    .unwrap();
    assert!(api.active_threads(TOKEN, GUILD).await.unwrap().is_empty());
    let t = api
        .create_thread(
            TOKEN,
            forum.id,
            &ChannelParams {
                kind: Some(channel_type::PUBLIC_THREAD),
                name: Some("t".into()),
                ..ChannelParams::default()
            },
            "r",
        )
        .await
        .unwrap();
    assert_eq!(api.active_threads(TOKEN, GUILD).await.unwrap()[0].id, t.id);
}

#[tokio::test]
async fn the_third_rename_in_the_window_is_rate_limited_and_the_bucket_blocks() {
    let (fake, api) = setup().await;
    let ch = api
        .create_channel(TOKEN, GUILD, &text("a"), "r")
        .await
        .unwrap();
    let rename = |n: &str| ChannelParams {
        name: Some(n.into()),
        ..ChannelParams::default()
    };
    api.edit_channel(TOKEN, ch.id, &rename("b"), "r")
        .await
        .unwrap();
    api.edit_channel(TOKEN, ch.id, &rename("c"), "r")
        .await
        .unwrap();
    let e = api
        .edit_channel(TOKEN, ch.id, &rename("d"), "r")
        .await
        .unwrap_err();
    assert!(matches!(e, CallError::RateLimited(_)));
    assert!(api.blocked_for(&channel_key(ch.id)).is_some());
    assert!(api.blocked_for(&channel_key(ch.id + 1)).is_none());
    assert_eq!(api.invalid_requests_within(Duration::from_secs(600)), 1);
    fake.reset_rename_window();
    assert_eq!(fake.channel(ch.id).unwrap()["name"], "c");
}

#[tokio::test]
async fn a_global_429_blocks_everything_and_a_wrong_token_is_401() {
    let (fake, api) = setup().await;
    fake.rate_limit_next(1, true);
    let e = api.guild_roles(TOKEN, GUILD).await.unwrap_err();
    assert!(matches!(e, CallError::RateLimited(_)));
    assert!(api.blocked_for(&channel_key(99)).is_some());
    let (_f2, api2) = setup().await;
    let e = api2.guild_roles("wrong", GUILD).await.unwrap_err();
    assert!(matches!(e, CallError::Status { status: 401, .. }));
    assert_eq!(api2.invalid_requests_within(Duration::from_secs(600)), 1);
    // The per-bridge breaker counts by bot token (a one-way key, never the token); the instance count is the sum.
    assert_eq!(
        api2.invalid_requests_for("wrong", Duration::from_secs(600)),
        1
    );
    assert_eq!(
        api2.invalid_requests_for(TOKEN, Duration::from_secs(600)),
        0
    );
}

#[tokio::test]
async fn bulk_positions_and_deletes() {
    let (fake, api) = setup().await;
    let a = api
        .create_channel(TOKEN, GUILD, &text("a"), "r")
        .await
        .unwrap();
    let b = api
        .create_channel(TOKEN, GUILD, &text("b"), "r")
        .await
        .unwrap();
    api.set_channel_positions(
        TOKEN,
        GUILD,
        &[
            ChannelPosition {
                id: a.id,
                position: 5,
                parent_id: Some(None),
                lock_permissions: false,
            },
            ChannelPosition {
                id: b.id,
                position: 1,
                parent_id: None,
                lock_permissions: false,
            },
        ],
        "r",
    )
    .await
    .unwrap();
    let all = api.guild_channels_all(TOKEN, GUILD).await.unwrap();
    assert_eq!(all.iter().find(|c| c.id == a.id).unwrap().position, 5);
    api.delete_channel(TOKEN, a.id, "r").await.unwrap();
    assert!(fake.channel(a.id).is_none());
    assert!(matches!(
        api.delete_channel(TOKEN, a.id, "r").await.unwrap_err(),
        CallError::Gone(404)
    ));
}

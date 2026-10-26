//! PAR-FU-003 (D-375, D-376): other servers' emoji, stickers and sounds are gated twice, by the
//! TARGET channel's USE_EXTERNAL_* bit and by USE_EXPRESSIONS_EXTERNALLY in the SOURCE server.
//! Runs over the settings test environment (messages and settings routes over one in-memory backend).

use super::soundboard_tests::{env_voice, wav_uri};
use super::*;
use sr_chat::rich_repo::{GuildEmojiRef, StickerRecord};
use sr_chat::settings::expressions::{Emoji, Sticker, StickerFormat};
use sr_chat::settings::repo::ExpressionRepository;
use sr_core::permissions::{Overwrite, OverwriteKind};
use sr_core::{EmojiId, Snowflake, StickerId};

const POG: u64 = 5_001;
const SPRITE: u64 = 5_002;
const APP_EMOJI: u64 = 5_003;
const HOME_EMOJI: u64 = 5_004;
const DM: u64 = 7_777;

fn member(guild: GuildId, user: u64) -> Member {
    Member {
        guild_id: guild,
        user_id: UserId::new(user),
        nick: None,
        role_ids: vec![],
        joined_at_ms: T0,
        communication_disabled_until_ms: None,
        pending: false,
        avatar_key: None,
        banner_key: None,
        bio: None,
        extras: Default::default(),
    }
}

/// A second server owned by OWNER where BOB is a plain member with the default @everyone set.
async fn source_server(e: &Env, process: u8) -> GuildId {
    let clock: Arc<dyn Clock> = e.clock.clone();
    let ids = SnowflakeGenerator::new(clock, 7, process).unwrap();
    let bp = plan_new_guild(UserId::new(OWNER), "Source", T0, || ids.generate()).unwrap();
    let id = bp.guild.id;
    e.backend.insert_guild(&bp).await.unwrap();
    e.backend.data().members.push(member(id, BOB));
    id
}

async fn emoji(e: &Env, guild: GuildId, id: u64, name: &str, available: bool, roles: Vec<RoleId>) {
    e.backend.data().guild_emojis.insert(
        id,
        GuildEmojiRef {
            id,
            guild_id: guild,
            available,
            role_ids: roles.clone(),
        },
    );
    e.repo
        .insert_emoji(&Emoji {
            id: EmojiId::new(id),
            guild_id: guild,
            name: name.into(),
            animated: false,
            available,
            role_ids: roles,
            creator_id: UserId::new(OWNER),
            media_key: format!("emoji/{id}.png"),
            created_at_ms: T0,
        })
        .await
        .unwrap();
}

async fn sticker(e: &Env, guild: GuildId, id: u64, name: &str) {
    e.backend.data().sticker_rows.insert(
        id,
        StickerRecord {
            id,
            guild_id: Some(guild),
            name: name.into(),
            format_type: 1,
            available: true,
        },
    );
    e.repo
        .insert_sticker(&Sticker {
            id: StickerId::new(id),
            guild_id: guild,
            name: name.into(),
            description: None,
            tags: "x".into(),
            format: StickerFormat::Png,
            available: true,
            creator_id: UserId::new(OWNER),
            media_key: format!("sticker/{id}.png"),
            created_at_ms: T0,
        })
        .await
        .unwrap();
}

/// Take USE_EXPRESSIONS_EXTERNALLY away from `@everyone` in `guild` (the owner keeps it).
fn revoke_source(e: &Env, guild: GuildId) {
    let mut d = e.backend.data();
    let role = d
        .roles
        .iter_mut()
        .find(|r| r.guild_id == guild && r.is_everyone())
        .unwrap();
    role.permissions
        .remove(Permissions::USE_EXPRESSIONS_EXTERNALLY);
}

/// Deny `bit` to `@everyone` on `channel`.
fn deny_in(e: &Env, channel: ChannelId, bit: Permissions) {
    e.backend
        .data()
        .overwrites
        .entry(channel)
        .or_default()
        .push(Overwrite {
            id: Snowflake::new(e.guild.get()),
            kind: OverwriteKind::Role,
            allow: Permissions::empty(),
            deny: bit,
        });
}

fn open_dm(e: &Env) -> ChannelId {
    let dm = ChannelId::new(DM);
    let mut d = e.backend.data();
    d.channels.insert(
        dm,
        sr_chat::Channel::new(dm, None, sr_chat::ChannelType::Dm),
    );
    d.recipients.insert((dm, UserId::new(OWNER)));
    d.recipients.insert((dm, UserId::new(BOB)));
    dm
}

async fn say(e: &Env, channel: ChannelId, who: &str, content: &str) -> (StatusCode, Value) {
    e.post(
        &format!("/channels/{channel}/messages"),
        who,
        json!({ "content": content }),
    )
    .await
}

fn token(name: &str, id: u64) -> String {
    format!("<:{name}:{id}>")
}

#[tokio::test]
async fn message_emoji_are_kept_only_when_both_gates_pass() {
    let e = env().await;
    let src = source_server(&e, 8).await;
    emoji(&e, src, POG, "pog", true, vec![]).await;
    let text = format!("hi {} there", token("pog", POG));

    // Both gates pass: the token stays.
    let (s, v) = say(&e, e.ch, "bob", &text).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["content"], text);

    // The target channel lacks USE_EXTERNAL_EMOJIS (bit 18): `:pog:` text, message goes through.
    deny_in(&e, e.ch, Permissions::USE_EXTERNAL_EMOJIS);
    let (s, v) = say(&e, e.ch, "bob", &text).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["content"], "hi :pog: there");
}

#[tokio::test]
async fn message_emoji_are_rewritten_when_the_source_gate_fails() {
    let e = env().await;
    let src = source_server(&e, 8).await;
    emoji(&e, src, POG, "pog", true, vec![]).await;
    let text = format!("{} and {}", token("pog", POG), "<a:dance:5001>");

    // The source's @everyone lacks bit 60.
    revoke_source(&e, src);
    let (s, v) = say(&e, e.ch, "bob", &text).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["content"], ":pog: and :dance:");
    // The owner of the source keeps it (owner bypass).
    let (_, v) = say(&e, e.ch, "owner", &token("pog", POG)).await;
    assert_eq!(v["content"], token("pog", POG));
}

#[tokio::test]
async fn message_emoji_are_rewritten_when_the_sender_left_the_source() {
    let e = env().await;
    let src = source_server(&e, 8).await;
    emoji(&e, src, POG, "pog", true, vec![]).await;
    let text = token("pog", POG);
    let (_, v) = say(&e, e.ch, "bob", &text).await;
    assert_eq!(v["content"], text);
    e.backend
        .data()
        .members
        .retain(|m| !(m.guild_id == src && m.user_id == UserId::new(BOB)));
    let (s, v) = say(&e, e.ch, "bob", &text).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["content"], ":pog:");
}

#[tokio::test]
async fn emoji_own_restrictions_apply() {
    let e = env().await;
    let src = source_server(&e, 8).await;
    let vip = RoleId::new(424_242);
    emoji(&e, src, POG, "pog", true, vec![vip]).await;
    emoji(&e, src, SPRITE, "sprite", false, vec![]).await;
    // Role-restricted and unavailable: both fail closed for a member without the role.
    let (_, v) = say(
        &e,
        e.ch,
        "bob",
        &format!("{}{}", token("pog", POG), token("sprite", SPRITE)),
    )
    .await;
    assert_eq!(v["content"], ":pog::sprite:");
    // With the role the restricted one passes.
    for m in e.backend.data().members.iter_mut() {
        if m.guild_id == src && m.user_id == UserId::new(BOB) {
            m.role_ids.push(vip);
        }
    }
    let (_, v) = say(&e, e.ch, "bob", &token("pog", POG)).await;
    assert_eq!(v["content"], token("pog", POG));
}

#[tokio::test]
async fn own_server_and_application_emoji_are_untouched() {
    let e = env().await;
    let src = source_server(&e, 8).await;
    emoji(&e, src, POG, "pog", true, vec![]).await;
    emoji(&e, e.guild, HOME_EMOJI, "home", true, vec![]).await;
    e.backend.data().app_emojis.insert(APP_EMOJI, 1);
    // Close every external door: bit 18 denied here and bit 60 revoked in the source.
    deny_in(&e, e.ch, Permissions::USE_EXTERNAL_EMOJIS);
    revoke_source(&e, src);
    let text = format!(
        "{} {} {} <:ghost:999999>",
        token("home", HOME_EMOJI),
        token("app", APP_EMOJI),
        token("pog", POG)
    );
    let (s, v) = say(&e, e.ch, "bob", &text).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        v["content"],
        format!(
            "{} {} :pog: <:ghost:999999>",
            token("home", HOME_EMOJI),
            token("app", APP_EMOJI)
        )
    );
}

#[tokio::test]
async fn in_a_dm_only_the_source_gate_applies() {
    let e = env().await;
    let src = source_server(&e, 8).await;
    emoji(&e, src, POG, "pog", true, vec![]).await;
    let dm = open_dm(&e);
    let text = token("pog", POG);
    let (s, v) = say(&e, dm, "bob", &text).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["content"], text);
    revoke_source(&e, src);
    let (_, v) = say(&e, dm, "bob", &text).await;
    assert_eq!(v["content"], ":pog:");
}

#[tokio::test]
async fn editing_applies_the_same_gates() {
    let e = env().await;
    let src = source_server(&e, 8).await;
    emoji(&e, src, POG, "pog", true, vec![]).await;
    let (_, m) = say(&e, e.ch, "bob", "plain").await;
    let path = format!("/channels/{}/messages/{}", e.ch, m["id"].as_str().unwrap());
    let (s, v) = e
        .patch(&path, "bob", json!({"content": token("pog", POG)}))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["content"], token("pog", POG));
    revoke_source(&e, src);
    let (s, v) = e
        .patch(
            &path,
            "bob",
            json!({"content": format!("again {}", token("pog", POG))}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["content"], "again :pog:");
}

#[tokio::test]
async fn stickers_of_other_servers_need_both_gates_else_unknown_sticker() {
    let e = env().await;
    let src = source_server(&e, 8).await;
    sticker(&e, src, 6_001, "wave").await;
    let send = |who: &'static str, channel: ChannelId| {
        let e = &e;
        async move {
            e.post(
                &format!("/channels/{channel}/messages"),
                who,
                json!({"sticker_ids": ["6001"]}),
            )
            .await
        }
    };
    let (s, v) = send("bob", e.ch).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["sticker_items"][0]["name"], "wave");

    // A DM only needs the source gate.
    let dm = open_dm(&e);
    assert_eq!(send("bob", dm).await.0, StatusCode::OK);

    // Target channel without bit 37.
    deny_in(&e, e.ch, Permissions::USE_EXTERNAL_STICKERS);
    let r = send("bob", e.ch).await;
    assert_eq!(r.0, StatusCode::BAD_REQUEST, "{}", r.1);
    assert_eq!(
        r.1["errors"]["sticker_ids"]["_errors"][0]["code"],
        "UNKNOWN_STICKER"
    );

    // Source without bit 60 (in a DM, where the target gate holds).
    revoke_source(&e, src);
    let r = send("bob", dm).await;
    assert_eq!(r.0, StatusCode::BAD_REQUEST, "{}", r.1);
    assert_eq!(
        r.1["errors"]["sticker_ids"]["_errors"][0]["code"],
        "UNKNOWN_STICKER"
    );
}

#[tokio::test]
async fn reactions_with_other_servers_emoji_are_403_when_a_gate_fails() {
    let e = env().await;
    let src = source_server(&e, 8).await;
    emoji(&e, src, POG, "pog", true, vec![]).await;
    let (_, m) = say(&e, e.ch, "owner", "react to me").await;
    let react = format!(
        "/channels/{}/messages/{}/reactions/pog:{POG}/@me",
        e.ch,
        m["id"].as_str().unwrap()
    );
    // Both gates pass.
    let (s, v) = e.call(Method::PUT, &react, Some("bob"), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    e.call(Method::DELETE, &react, Some("bob"), None).await;

    // Target channel lacks bit 18.
    deny_in(&e, e.ch, Permissions::USE_EXTERNAL_EMOJIS);
    let r = e.call(Method::PUT, &react, Some("bob"), None).await;
    assert_eq!(code_of(&r), (StatusCode::FORBIDDEN, Some(50013)));
    // Unicode reactions are not affected.
    let uni = format!(
        "/channels/{}/messages/{}/reactions/%F0%9F%91%8D/@me",
        e.ch,
        m["id"].as_str().unwrap()
    );
    let r = e.call(Method::PUT, &uni, Some("bob"), None).await;
    assert_eq!(r.0, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn reactions_are_403_when_the_source_gate_fails() {
    let e = env().await;
    let src = source_server(&e, 8).await;
    emoji(&e, src, POG, "pog", true, vec![]).await;
    let (_, m) = say(&e, e.ch, "owner", "react to me").await;
    let react = format!(
        "/channels/{}/messages/{}/reactions/pog:{POG}/@me",
        e.ch,
        m["id"].as_str().unwrap()
    );
    revoke_source(&e, src);
    let r = e.call(Method::PUT, &react, Some("bob"), None).await;
    assert_eq!(code_of(&r), (StatusCode::FORBIDDEN, Some(50013)));
    // The owner of the source passes the gate.
    let r = e.call(Method::PUT, &react, Some("owner"), None).await;
    assert_eq!(r.0, StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn sounds_of_other_servers_need_both_gates_else_403() {
    let (e, plays) = env_voice().await;
    let src = source_server(&e, 8).await;
    let (s, v) = e
        .post(
            &format!("/guilds/{src}/soundboard-sounds"),
            "owner",
            json!({"name": "ding", "sound": wav_uri(300)}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    let sid = v["sound_id"].as_str().unwrap().to_owned();
    let play = format!("/channels/{}/send-soundboard-sound", e.voice);
    let body = json!({"sound_id": sid, "source_guild_id": src.to_string()});

    let (s, v) = e.post(&play, "bob", body.clone()).await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    assert_eq!(plays.seen.lock().unwrap().len(), 1);

    // The target voice channel lacks USE_EXTERNAL_SOUNDS (bit 45).
    deny_in(&e, e.voice, Permissions::USE_EXTERNAL_SOUNDS);
    let r = e.post(&play, "bob", body.clone()).await;
    assert_eq!(code_of(&r), (StatusCode::FORBIDDEN, Some(50013)));
    e.backend.data().overwrites.remove(&e.voice);

    // The source server does not let bob's roles take its sounds out.
    revoke_source(&e, src);
    let r = e.post(&play, "bob", body.clone()).await;
    assert_eq!(code_of(&r), (StatusCode::FORBIDDEN, Some(50013)));
    assert_eq!(plays.seen.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn the_listing_has_one_entry_per_server_with_the_source_gate_per_server() {
    let e = env().await;
    let src = source_server(&e, 8).await;
    let vip = RoleId::new(424_242);
    emoji(&e, src, POG, "pog", true, vec![]).await;
    emoji(&e, src, SPRITE, "sprite", false, vec![]).await;
    emoji(&e, src, APP_EMOJI, "vip_only", true, vec![vip]).await;
    emoji(&e, e.guild, HOME_EMOJI, "home", true, vec![]).await;
    sticker(&e, src, 6_001, "wave").await;

    let (s, v) = e.get("/users/@me/expressions", "bob").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let list = v.as_array().unwrap();
    assert_eq!(list.len(), 2, "{v}");
    let of = |g: GuildId| {
        list.iter()
            .find(|x| x["guild"]["id"] == g.to_string())
            .unwrap_or_else(|| panic!("no entry for {g}: {v}"))
    };
    let s_src = of(src);
    assert_eq!(s_src["guild"]["name"], "Source");
    assert_eq!(s_src["usable_externally"], true);
    // Unavailable and role-restricted emoji are filtered out for this caller.
    let names: Vec<_> = s_src["emojis"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["pog"]);
    assert_eq!(s_src["stickers"][0]["name"], "wave");
    assert_eq!(of(e.guild)["emojis"][0]["name"], "home");

    // usable_externally is the source gate per server; the lists stay (usable inside the server).
    revoke_source(&e, src);
    let (_, v) = e.get("/users/@me/expressions", "bob").await;
    let off = v
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["guild"]["id"] == src.to_string())
        .unwrap();
    assert_eq!(off["usable_externally"], false);
    assert_eq!(off["emojis"].as_array().unwrap().len(), 1);
    let home = v
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["guild"]["id"] == e.guild.to_string())
        .unwrap();
    assert_eq!(home["usable_externally"], true);
}

#[tokio::test]
async fn a_member_of_no_server_gets_an_empty_list_not_someone_elses() {
    let e = env().await;
    let src = source_server(&e, 8).await;
    emoji(&e, src, POG, "pog", true, vec![]).await;
    let (s, v) = e.get("/users/@me/expressions", "carol").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v, json!([]));
    // And no credentials, no list.
    let (s, _) = e
        .call(Method::GET, "/users/@me/expressions", None, None)
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

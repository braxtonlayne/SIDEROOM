//! The server 2FA requirement on elevated message routes, for browser sessions and for bots whose
//! application owner has or lacks a second factor (G-94, D-223).

use super::tests::*;
use axum::http::StatusCode;
use serde_json::json;
use sr_chat::Role;
use sr_core::{Permissions, RoleId, UserId};

const OWNER_SESSION: i64 = 100;

async fn enrol(e: &Env, user: i64) {
    use crate::routes::auth::AuthStore;
    let store = e.auth.as_ref().unwrap();
    store.totp_put_pending(user, &[7u8; 20]).await.unwrap();
    store.totp_confirm(user, 0).await.unwrap();
}

async fn fresh(e: &Env, session: i64) {
    use crate::routes::auth::AuthStore;
    e.auth
        .as_ref()
        .unwrap()
        .session_mark_mfa_verified(session, T0 / 1000)
        .await
        .unwrap();
}

/// A message by BOB in the guild's channel.
async fn say(e: &Env, text: &str) -> String {
    let (s, v) = e
        .post(
            Who::Session(BOB),
            &format!("/channels/{}/messages", e.ch),
            json!({ "content": text }),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    v["id"].as_str().unwrap().to_owned()
}

/// A guild that requires 2FA and an installed bot that may manage messages.
async fn world() -> (Env, String) {
    let e = env_build(true, None, true).await;
    e.backend.data().guilds.get_mut(&e.guild).unwrap().mfa_level = 1;
    let (id, bot, token, _) = e.new_app().await;
    e.configure(&id).await;
    e.install(&id, &bot, 0).await;
    let rid = RoleId::new(777);
    let bot_id = UserId::new(bot.parse().unwrap());
    {
        let mut d = e.backend.data();
        d.roles.push(Role {
            id: rid,
            guild_id: e.guild,
            name: "Janitor".into(),
            color: 0,
            hoist: false,
            position: 1,
            permissions: Permissions::MANAGE_MESSAGES
                | Permissions::VIEW_CHANNEL
                | Permissions::READ_MESSAGE_HISTORY,
            mentionable: false,
            managed: false,
            unicode_emoji: None,
            icon_key: None,
        });
        for m in d.members.iter_mut().filter(|m| m.user_id == bot_id) {
            m.role_ids.push(rid);
        }
    }
    (e, token)
}

fn refusal(s: StatusCode, v: &serde_json::Value) -> (u16, i64) {
    (s.as_u16(), code_of(v))
}

#[tokio::test]
async fn elevated_message_actions_need_a_second_factor_on_a_2fa_server() {
    let (e, token) = world().await;
    let ch = e.ch;
    let theirs = say(&e, "one").await;
    let pin = |who: Who<'_>| (who, format!("/channels/{ch}/pins/{theirs}"));

    // Bots first: the application owner has no second factor -> 900123.
    let (who, path) = pin(Who::Bot(&token));
    let (s, v) = e.put(who, &path, json!({})).await;
    assert_eq!(refusal(s, &v), (403, 900123), "{v}");
    let (s, v) = e
        .delete(
            Who::Bot(&token),
            &format!("/channels/{ch}/messages/{theirs}"),
            None,
        )
        .await;
    assert_eq!(refusal(s, &v), (403, 900123), "{v}");
    let b = vec![say(&e, "a").await, say(&e, "b").await];
    let (s, v) = e
        .post(
            Who::Bot(&token),
            &format!("/channels/{ch}/messages/bulk-delete"),
            json!({ "messages": b }),
        )
        .await;
    assert_eq!(refusal(s, &v), (403, 900123), "{v}");

    // A session without any second factor gets the enrolment error, not the re-auth one.
    let (s, v) = e
        .delete(
            Who::Session(OWNER),
            &format!("/channels/{ch}/messages/{theirs}"),
            None,
        )
        .await;
    assert_eq!(refusal(s, &v), (403, 900123), "{v}");

    // A member deleting their own message is not an elevated action.
    let own = say(&e, "mine").await;
    let (s, v) = e
        .delete(
            Who::Session(BOB),
            &format!("/channels/{ch}/messages/{own}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");

    // Enrolled but not proved on this session: 900004; the bot's owner now has one: allowed.
    enrol(&e, OWNER).await;
    let (s, v) = e
        .delete(
            Who::Session(OWNER),
            &format!("/channels/{ch}/messages/{theirs}"),
            None,
        )
        .await;
    assert_eq!(refusal(s, &v), (403, 900004), "{v}");
    let (s, v) = e
        .put(
            Who::Session(OWNER),
            &format!("/channels/{ch}/pins/{theirs}"),
            json!({}),
        )
        .await;
    assert_eq!(refusal(s, &v), (403, 900004), "{v}");
    let (who, path) = pin(Who::Bot(&token));
    let (s, v) = e.put(who, &path, json!({})).await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let (s, v) = e
        .delete(
            Who::Bot(&token),
            &format!("/channels/{ch}/pins/{theirs}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");

    // Proved within the window: the session may moderate too.
    fresh(&e, OWNER_SESSION).await;
    let (s, v) = e
        .post(
            Who::Session(OWNER),
            &format!("/channels/{ch}/messages/bulk-delete"),
            json!({ "messages": b }),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let (s, v) = e
        .delete(
            Who::Session(OWNER),
            &format!("/channels/{ch}/messages/{theirs}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
}

#[tokio::test]
async fn a_server_without_the_requirement_is_not_gated() {
    let (e, token) = world().await;
    e.backend.data().guilds.get_mut(&e.guild).unwrap().mfa_level = 0;
    let ch = e.ch;
    let m = say(&e, "x").await;
    let (s, v) = e
        .put(
            Who::Bot(&token),
            &format!("/channels/{ch}/pins/{m}"),
            json!({}),
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let (s, v) = e
        .delete(
            Who::Session(OWNER),
            &format!("/channels/{ch}/messages/{m}"),
            None,
        )
        .await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
}

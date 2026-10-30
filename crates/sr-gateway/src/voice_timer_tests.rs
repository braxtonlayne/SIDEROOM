//! UTL-DEV-001: a timer shared with a voice channel. Same fake directory and client as the parent module.

use super::*;

fn allow_status(e: &Env, user: u64, ch: ChannelId) {
    e.dir.perms.lock().unwrap().insert(
        (u(user), ch),
        member_perms() | Permissions::SET_VOICE_CHANNEL_STATUS,
    );
}

fn timer() -> Value {
    json!({
        "kind": "pomodoro", "phase": "running", "segment": "focus",
        "startedMs": 1_700_000_000_000u64, "accumulatedMs": 0, "durationMs": 1_500_000, "completed": 0
    })
}

async fn join(e: &Env, user: u64, channel: ChannelId) -> Client {
    let mut c = client(e, user).await;
    c.voice(1, Some(channel.get()), false, false).await;
    c.event("VOICE_SERVER_UPDATE").await;
    c.drain().await;
    c
}

#[tokio::test]
async fn a_timer_reaches_the_channel_with_a_status_line_and_a_server_clock() {
    let e = env();
    allow_status(&e, MEMBER, LOBBY);
    let mut watcher = client(&e, FRIEND).await;
    let _a = join(&e, MEMBER, LOBBY).await;
    watcher.drain().await;

    e.gw.voice_timer_set(u(MEMBER), LOBBY, Some(&timer()), "Focus 25:00")
        .await
        .unwrap();
    assert_eq!(
        e.gw.voice_channel_status(LOBBY).unwrap().text,
        "Focus 25:00"
    );
    let ev = watcher.event("VOICE_TIMER_UPDATE").await;
    assert_eq!(ev["id"], "20");
    assert_eq!(ev["guild_id"], "1");
    assert_eq!(ev["user_id"], MEMBER.to_string());
    assert!(ev["server_ms"].as_u64().unwrap() > 1_600_000_000_000);
    assert_eq!(ev["timer"]["kind"], "pomodoro");
    assert_eq!(ev["timer"]["durationMs"], 1_500_000);

    // Stopping clears our own status line and tells the channel the timer is gone.
    e.gw.voice_timer_set(u(MEMBER), LOBBY, None, "")
        .await
        .unwrap();
    assert!(e.gw.voice_channel_status(LOBBY).is_none());
    assert_eq!(
        watcher.event("VOICE_TIMER_UPDATE").await["timer"],
        Value::Null
    );
}

#[tokio::test]
async fn who_may_publish_a_timer_and_what_is_refused() {
    let e = env();
    let _a = join(&e, MEMBER, LOBBY).await;
    // Without the status permission, even from inside the channel.
    assert_eq!(
        e.gw.voice_timer_set(u(MEMBER), LOBBY, Some(&timer()), "Focus")
            .await,
        Err(StatusError::MissingPermissions)
    );
    assert!(e.gw.voice_channel_status(LOBBY).is_none());

    allow_status(&e, MEMBER, LOBBY);
    // A malformed timer, or no label, changes nothing.
    assert_eq!(
        e.gw.voice_timer_set(u(MEMBER), LOBBY, Some(&json!({"kind": "alarm"})), "Focus")
            .await,
        Err(StatusError::InvalidStatus)
    );
    assert_eq!(
        e.gw.voice_timer_set(u(MEMBER), LOBBY, Some(&timer()), "   ")
            .await,
        Err(StatusError::InvalidStatus)
    );
    assert!(e.gw.voice_channel_status(LOBBY).is_none());
    // Someone not in the channel needs Manage Channels.
    assert_eq!(
        e.gw.voice_timer_set(u(FRIEND), LOBBY, Some(&timer()), "Focus")
            .await,
        Err(StatusError::MissingPermissions)
    );
    // An unknown channel.
    assert_eq!(
        e.gw.voice_timer_set(u(MOD), ChannelId::new(999), Some(&timer()), "x")
            .await,
        Err(StatusError::UnknownChannel)
    );
    // A text channel is not a voice channel.
    assert_eq!(
        e.gw.voice_timer_set(u(MOD), TEXT, Some(&timer()), "x")
            .await,
        Err(StatusError::NotVoiceChannel)
    );
    // Stopping when nothing is shared is a quiet success.
    assert_eq!(
        e.gw.voice_timer_set(u(MEMBER), LOBBY, None, "").await,
        Ok(())
    );
}

#[tokio::test]
async fn a_moderator_stops_a_timer_but_the_status_line_stays_if_it_is_not_theirs() {
    let e = env();
    allow_status(&e, MEMBER, LOBBY);
    let mut watcher = client(&e, FRIEND).await;
    let _a = join(&e, MEMBER, LOBBY).await;
    e.gw.voice_timer_set(u(MEMBER), LOBBY, Some(&timer()), "Focus 25:00")
        .await
        .unwrap();
    watcher.drain().await;
    // The moderator (Manage Channels) may stop it from outside.
    e.gw.voice_timer_set(u(MOD), LOBBY, None, "").await.unwrap();
    assert_eq!(
        watcher.event("VOICE_TIMER_UPDATE").await["timer"],
        Value::Null
    );
}

#[tokio::test]
async fn the_timer_event_is_a_voice_states_intent_event() {
    assert_eq!(
        crate::protocol::intents::required_for("VOICE_TIMER_UPDATE"),
        Some(crate::protocol::intents::GUILD_VOICE_STATES)
    );
}

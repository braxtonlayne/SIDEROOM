//! Soundboard endpoint tests (VOX-SB-001) over the settings test environment.

use super::*;
use crate::routes::voice::{
    SoundEffect, TurnCredentialsObject, VoiceBackend, VoiceError, VoicePatch,
};
use async_trait::async_trait;
use sr_chat::settings::soundboard::{DEFAULT_SOUNDS, render_default};
use std::sync::Mutex;

#[derive(Default)]
pub(super) struct Plays {
    pub(super) seen: Mutex<Vec<(UserId, GuildId, ChannelId, SoundEffect)>>,
    fail: Mutex<Option<VoiceError>>,
}

#[async_trait]
impl VoiceBackend for Plays {
    fn turn_credentials(&self, _user: UserId) -> Option<TurnCredentialsObject> {
        None
    }

    async fn moderate(
        &self,
        _actor: UserId,
        _guild: GuildId,
        _target: UserId,
        _patch: VoicePatch,
        _reason: Option<String>,
    ) -> Result<(), VoiceError> {
        Ok(())
    }

    async fn soundboard_play(
        &self,
        actor: UserId,
        guild: GuildId,
        channel: ChannelId,
        effect: SoundEffect,
    ) -> Result<(), VoiceError> {
        self.seen
            .lock()
            .unwrap()
            .push((actor, guild, channel, effect));
        self.fail.lock().unwrap().map_or(Ok(()), Err)
    }
}

fn wav_bytes(ms: usize) -> Vec<u8> {
    let rate = 8_000usize;
    let n = rate * ms / 1000;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + n * 2) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(rate as u32).to_le_bytes());
    out.extend_from_slice(&((rate * 2) as u32).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&((n * 2) as u32).to_le_bytes());
    out.resize(out.len() + n * 2, 7);
    out
}

pub(super) fn wav_uri(ms: usize) -> String {
    format!("data:audio/wav;base64,{}", BASE64.encode(&wav_bytes(ms)))
}

pub(super) async fn env_voice() -> (Env, Arc<Plays>) {
    let plays = Arc::new(Plays::default());
    let e = env_with(Opts {
        voice: Some(plays.clone()),
        ..Opts::default()
    })
    .await;
    (e, plays)
}

#[tokio::test]
async fn default_sounds_are_listed_and_playable() {
    let e = env().await;
    let (s, v) = e.get("/soundboard-default-sounds", "bob").await;
    assert_eq!(s, StatusCode::OK);
    let list = v.as_array().unwrap();
    assert_eq!(list.len(), 8);
    assert_eq!(list[0]["sound_id"], "1");
    assert_eq!(list[0]["available"], true);
    assert!(list[0].get("guild_id").is_none());

    let req = Request::builder()
        .uri("/api/v1/soundboard-default-sounds/4/audio")
        .header("authorization", "Bot bob")
        .body(Body::empty())
        .unwrap();
    let resp = e.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["content-type"], "audio/wav");
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.to_vec(), render_default(&DEFAULT_SOUNDS[3]));

    let (s, v) = e.get("/soundboard-default-sounds/99/audio", "bob").await;
    assert_eq!(code_of(&(s, v)), (StatusCode::NOT_FOUND, Some(10097)));
    let (s, _) = e
        .call(Method::GET, "/soundboard-default-sounds", None, None)
        .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn sounds_are_uploaded_listed_changed_and_deleted() {
    let e = env().await;
    let base = e.g("/soundboard-sounds");
    // A plain member has no Create/Manage Expressions.
    let r = e
        .post(&base, "bob", json!({"name": "ding", "sound": wav_uri(500)}))
        .await;
    assert_eq!(code_of(&r), (StatusCode::FORBIDDEN, Some(50013)));
    // A stranger cannot even see the server.
    let r = e.get(&base, "carol").await;
    assert_eq!(code_of(&r), (StatusCode::FORBIDDEN, Some(50001)));

    let (s, v) = e
        .post(
            &base,
            "mod",
            json!({"name": "ding", "sound": wav_uri(500), "volume": 0.6, "emoji_name": "\u{1F514}"}),
        )
        .await;
    assert_eq!(s, StatusCode::CREATED, "{v}");
    assert_eq!(v["name"], "ding");
    assert_eq!(v["volume"], 0.6);
    assert_eq!(v["emoji_name"], "\u{1F514}");
    assert_eq!(v["guild_id"], e.guild.to_string());
    assert_eq!(v["available"], true);
    assert_eq!(v["user"]["id"], MOD.to_string());
    let sid = v["sound_id"].as_str().unwrap().to_owned();

    // Any member lists and reads it.
    let (s, v) = e.get(&base, "bob").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["items"].as_array().unwrap().len(), 1);
    let (s, v) = e.get(&format!("{base}/{sid}"), "bob").await;
    assert_eq!((s, v["name"].clone()), (StatusCode::OK, json!("ding")));

    // The audio comes back byte for byte, with the type it was sniffed as.
    let req = Request::builder()
        .uri(format!("/api/v1{base}/{sid}/audio"))
        .header("authorization", "Bot bob")
        .body(Body::empty())
        .unwrap();
    let resp = e.app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()["content-type"], "audio/wav");
    assert_eq!(
        resp.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
        wav_bytes(500)
    );
    assert_eq!(e.storage.list("sound/").await.unwrap().len(), 1);

    // Rename, change the volume, clear the emoji.
    let (s, v) = e
        .patch(
            &format!("{base}/{sid}"),
            "mod",
            json!({"name": "dong", "volume": 0.25, "emoji_name": null}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        (v["name"].clone(), v["volume"].clone()),
        (json!("dong"), json!(0.25))
    );
    assert!(v["emoji_name"].is_null());
    let r = e
        .patch(&format!("{base}/{sid}"), "bob", json!({"name": "mine"}))
        .await;
    assert_eq!(code_of(&r), (StatusCode::FORBIDDEN, Some(50013)));

    // Every change is announced with the full list.
    let names = e.events.names();
    assert_eq!(
        names
            .iter()
            .filter(|n| **n == "GUILD_SOUNDBOARD_SOUNDS_UPDATE")
            .count(),
        2
    );

    // Delete removes the file too.
    let r = e.delete(&format!("{base}/{sid}"), "bob").await;
    assert_eq!(code_of(&r), (StatusCode::FORBIDDEN, Some(50013)));
    let (s, _) = e.delete(&format!("{base}/{sid}"), "mod").await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    assert!(e.storage.list("sound/").await.unwrap().is_empty());
    let r = e.get(&format!("{base}/{sid}"), "bob").await;
    assert_eq!(code_of(&r), (StatusCode::NOT_FOUND, Some(10097)));
    let (s, v) = e.get(&base, "bob").await;
    assert_eq!(
        (s, v["items"].as_array().unwrap().len()),
        (StatusCode::OK, 0)
    );
}

#[tokio::test]
async fn bad_uploads_are_refused_before_storage() {
    let e = env_with(Opts {
        limits: Some(ExpressionLimits {
            max_soundboard_sounds: 1,
            ..ExpressionLimits::default()
        }),
        ..Opts::default()
    })
    .await;
    let base = e.g("/soundboard-sounds");
    let mut cases = vec![
        // Not audio.
        json!({"name": "text", "sound": png_uri(10)}),
        json!({"name": "text", "sound": "not a data uri"}),
        // Longer than five seconds.
        json!({"name": "long", "sound": wav_uri(5_500)}),
        // Bad name and volume.
        json!({"name": "x", "sound": wav_uri(300)}),
        json!({"name": "loud", "sound": wav_uri(300), "volume": 1.5}),
        // An emoji that is not this server's.
        json!({"name": "emoji", "sound": wav_uri(300), "emoji_id": "12345"}),
        // Both kinds of emoji at once.
        json!({"name": "both", "sound": wav_uri(300), "emoji_id": "12345", "emoji_name": "x"}),
    ];
    // Over the 512 KiB cap (a 6 s clip at a high rate is not 5 s, so use raw padding).
    let mut big = wav_bytes(300);
    big.resize(600 * 1024, 0);
    cases.push(
        json!({"name": "big", "sound": format!("data:audio/wav;base64,{}", BASE64.encode(&big))}),
    );
    for body in cases {
        let r = e.post(&base, "mod", body.clone()).await;
        assert_eq!(
            code_of(&r),
            (StatusCode::BAD_REQUEST, Some(50035)),
            "{body} -> {r:?}"
        );
    }
    assert!(e.storage.list("sound/").await.unwrap().is_empty());

    // Slots: one sound fits, the next is the 30045 error.
    assert_eq!(
        e.post(&base, "mod", json!({"name": "one", "sound": wav_uri(300)}))
            .await
            .0,
        StatusCode::CREATED
    );
    let r = e
        .post(&base, "mod", json!({"name": "two", "sound": wav_uri(300)}))
        .await;
    assert_eq!(code_of(&r), (StatusCode::BAD_REQUEST, Some(30045)));
    assert_eq!(e.storage.list("sound/").await.unwrap().len(), 1);
}

#[tokio::test]
async fn playing_a_sound_checks_permissions_then_asks_the_voice_system() {
    let (e, plays) = env_voice().await;
    let base = e.g("/soundboard-sounds");
    let (_, v) = e
        .post(
            &base,
            "mod",
            json!({"name": "ding", "sound": wav_uri(300), "volume": 0.5, "emoji_name": "\u{1F514}"}),
        )
        .await;
    let sid = v["sound_id"].as_str().unwrap().to_owned();
    let play = format!("/channels/{}/send-soundboard-sound", e.voice);

    // A default sound.
    let (s, v) = e.post(&play, "bob", json!({"sound_id": "3"})).await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    // The server's own sound carries its volume and emoji.
    let (s, _) = e.post(&play, "bob", json!({"sound_id": sid})).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    {
        let seen = plays.seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].0, UserId::new(BOB));
        assert_eq!(seen[0].1, e.guild);
        assert_eq!(seen[0].2, e.voice);
        assert_eq!(seen[0].3.sound_id, "3");
        assert_eq!(seen[0].3.volume, 1.0);
        assert_eq!(seen[1].3.sound_id, sid);
        assert_eq!(seen[1].3.volume, 0.5);
        assert_eq!(seen[1].3.emoji_name.as_deref(), Some("\u{1F514}"));
    }

    // Unknown sounds, text channels and strangers never reach the voice system.
    let r = e.post(&play, "bob", json!({"sound_id": "424242"})).await;
    assert_eq!(code_of(&r), (StatusCode::NOT_FOUND, Some(10097)));
    let r = e
        .post(
            &format!("/channels/{}/send-soundboard-sound", e.ch),
            "bob",
            json!({"sound_id": "3"}),
        )
        .await;
    assert_eq!(code_of(&r), (StatusCode::BAD_REQUEST, Some(50035)));
    let r = e.post(&play, "carol", json!({"sound_id": "3"})).await;
    assert_eq!(r.0, StatusCode::FORBIDDEN);
    let r = e.post(&play, "bob", json!({"sound_id": "nope"})).await;
    assert_eq!(code_of(&r), (StatusCode::BAD_REQUEST, Some(50035)));
    // Another server's sound needs a server that exists and that the member is in.
    let r = e
        .post(
            &play,
            "bob",
            json!({"sound_id": sid, "source_guild_id": "999"}),
        )
        .await;
    assert_eq!(code_of(&r), (StatusCode::NOT_FOUND, Some(10004)));
    assert_eq!(plays.seen.lock().unwrap().len(), 2);

    // The voice system's refusals become the matching errors.
    for (err, status, code) in [
        (VoiceError::NotInVoice, StatusCode::BAD_REQUEST, 40032),
        (VoiceError::Silenced, StatusCode::CONFLICT, 900_010),
        (VoiceError::Cooldown, StatusCode::TOO_MANY_REQUESTS, 900_012),
        (VoiceError::Unavailable, StatusCode::FORBIDDEN, 900_001),
    ] {
        *plays.fail.lock().unwrap() = Some(err);
        let r = e.post(&play, "bob", json!({"sound_id": "3"})).await;
        assert_eq!(code_of(&r), (status, Some(code)), "{err:?}");
    }
}

#[tokio::test]
async fn the_soundboard_needs_its_permissions() {
    let (e, plays) = env_voice().await;
    let play = format!("/channels/{}/send-soundboard-sound", e.voice);
    // Take Use Soundboard away from everyone in the voice channel.
    e.backend
        .data()
        .overwrites
        .entry(e.voice)
        .or_default()
        .push(sr_core::permissions::Overwrite {
            id: sr_core::Snowflake::new(e.guild.get()),
            kind: sr_core::permissions::OverwriteKind::Role,
            allow: Permissions::empty(),
            deny: Permissions::USE_SOUNDBOARD,
        });
    let r = e.post(&play, "bob", json!({"sound_id": "3"})).await;
    assert_eq!(code_of(&r), (StatusCode::FORBIDDEN, Some(50013)));
    assert!(plays.seen.lock().unwrap().is_empty());
}

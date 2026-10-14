//! Tests for the age gate, AFK, sender allowances, Go Live streams, stage channels and DM
//! calls (VOX-SIG-003, 008, 009, 010; VOX-STG-001). Same fake directory and client as the
//! parent module.

use super::*;

async fn join(e: &Env, user: u64, channel: ChannelId) -> Client {
    let mut c = client(e, user).await;
    c.voice(1, Some(channel.get()), false, false).await;
    c.event("VOICE_SERVER_UPDATE").await;
    c.drain().await;
    c
}

fn set_limits(e: &Env, senders: u16, viewers: u16) {
    let mut l = e.dir.default_limits.lock().unwrap();
    l.video_senders = senders;
    l.stream_viewers = viewers;
}

async fn error_code(c: &mut Client) -> Value {
    c.event("SR_VOICE_ERROR").await
}

// ---- age gate and AFK (VOX-SIG-003) ---------------------------------------------------------

#[tokio::test]
async fn age_restricted_voice_channels_need_the_18_plus_answer() {
    let e = env();
    let mut a = client(&e, MEMBER).await;
    a.voice(1, Some(ADULT.get()), false, false).await;
    let err = error_code(&mut a).await;
    assert_eq!(err["code"], "age_confirmation_required");
    assert!(e.gw.voice_state(u(MEMBER)).is_none());

    // After the one-time answer the retry succeeds.
    e.dir.adults.lock().unwrap().push(u(MEMBER));
    a.voice(1, Some(ADULT.get()), false, false).await;
    a.event("VOICE_SERVER_UPDATE").await;
    assert_eq!(e.gw.voice_state(u(MEMBER)).unwrap().channel_id, ADULT);
}

#[tokio::test(start_paused = true)]
async fn idle_members_are_moved_to_the_afk_channel() {
    let e = env();
    *e.dir.afk.lock().unwrap() = VoiceGuildInfo {
        afk_channel: Some(AFK),
        afk_timeout_secs: 10,
    };
    let mut a = join(&e, MEMBER, LOBBY).await;
    let mut talker = join(&e, FRIEND, LOBBY).await;
    e.relay.take();

    tokio::time::advance(Duration::from_secs(6)).await;
    e.gw.sweep().await;
    assert_eq!(e.gw.voice_state(u(MEMBER)).unwrap().channel_id, LOBBY);

    // The SFU heard FRIEND speak, so only MEMBER goes idle.
    e.gw.voice_activity(u(FRIEND), LOBBY);
    tokio::time::advance(Duration::from_secs(6)).await;
    e.gw.sweep().await;

    let moved = e.gw.voice_state(u(MEMBER)).unwrap();
    assert_eq!(moved.channel_id, AFK);
    assert!(moved.afk, "the AFK channel listens only");
    assert_eq!(e.gw.voice_state(u(FRIEND)).unwrap().channel_id, LOBBY);

    // MEMBER reconnects to the AFK room with a token that cannot speak.
    let srv = a.event("VOICE_SERVER_UPDATE").await;
    assert_eq!(srv["channel_id"], "28");
    let claims = TokenSigner::new(SECRET.to_vec())
        .verify(srv["token"].as_str().unwrap(), unix_now())
        .unwrap();
    assert!(!claims.speak);
    let seen = talker.drain().await;
    assert!(
        seen.iter()
            .any(|v| v["t"] == "VOICE_STATE_UPDATE" && v["d"]["channel_id"] == "28")
    );
    assert!(e.relay.take().contains(&VoiceCommand::Disconnect {
        channel: LOBBY,
        user: u(MEMBER)
    }));

    // Already in the AFK channel: not moved again, and no timer applies.
    tokio::time::advance(Duration::from_secs(30)).await;
    e.gw.sweep().await;
    assert_eq!(e.gw.voice_state(u(MEMBER)).unwrap().channel_id, AFK);
}

#[tokio::test(start_paused = true)]
async fn a_zero_afk_timeout_or_no_afk_channel_moves_nobody() {
    let e = env();
    let _a = join(&e, MEMBER, LOBBY).await;
    // No AFK channel configured.
    tokio::time::advance(Duration::from_secs(40)).await;
    e.gw.sweep().await;
    assert_eq!(e.gw.voice_state(u(MEMBER)).unwrap().channel_id, LOBBY);
    // A channel but timeout 0.
    *e.dir.afk.lock().unwrap() = VoiceGuildInfo {
        afk_channel: Some(AFK),
        afk_timeout_secs: 0,
    };
    e.gw.sweep().await;
    assert_eq!(e.gw.voice_state(u(MEMBER)).unwrap().channel_id, LOBBY);
}

// ---- sender allowance (VOX-SIG-009) -----------------------------------------------------------

#[tokio::test]
async fn the_sixth_camera_is_refused_and_a_patron_is_accepted_at_count_five() {
    let e = env();
    // The free limit is five; use a lobby of three members and a limit of two to stay small.
    set_limits(&e, 2, 25);
    let mut a = client(&e, MEMBER).await;
    a.voice_video(1, LOBBY, true).await;
    a.event("VOICE_SERVER_UPDATE").await;
    let mut b = client(&e, FRIEND).await;
    b.voice_video(1, LOBBY, true).await;
    b.event("VOICE_SERVER_UPDATE").await;
    assert!(e.gw.voice_state(u(MEMBER)).unwrap().self_video);
    assert!(e.gw.voice_state(u(FRIEND)).unwrap().self_video);

    // The third sender joins audio-only and is told the numbers for the toast.
    let mut m = client(&e, MOD).await;
    m.voice_video(1, LOBBY, true).await;
    let err = error_code(&mut m).await;
    assert_eq!(err["code"], "video_limit_reached");
    assert_eq!(err["limit"], 2);
    assert_eq!(err["count"], 2);
    let st = e.gw.voice_state(u(MOD)).unwrap();
    assert!(!st.self_video, "the join stays, the camera does not");

    // Turning it on later is refused the same way.
    m.voice_video(1, LOBBY, true).await;
    assert_eq!(error_code(&mut m).await["code"], "video_limit_reached");

    // A Patron does not count and is not refused, at any count (D-052).
    let mut patron = e.dir.default_limits.lock().unwrap().clone();
    patron.exempt = true;
    e.dir.limits.lock().unwrap().insert(u(MOD), patron);
    m.voice_video(1, LOBBY, true).await;
    m.drain().await;
    let st = e.gw.voice_state(u(MOD)).unwrap();
    assert!(st.self_video && st.exempt);
    // The patron's camera does not use up a seat: a member can still stop and restart theirs.
    assert_eq!(e.gw.sender_count(LOBBY, u(MEMBER)), 1);
    a.voice_video(1, LOBBY, false).await;
    a.drain().await;
    a.voice_video(1, LOBBY, true).await;
    a.drain().await;
    assert!(e.gw.voice_state(u(MEMBER)).unwrap().self_video);
}

#[tokio::test]
async fn lowering_a_limit_never_kicks_a_sender_it_only_blocks_new_ones() {
    let e = env();
    set_limits(&e, 2, 25);
    let mut a = client(&e, MEMBER).await;
    a.voice_video(1, LOBBY, true).await;
    a.event("VOICE_SERVER_UPDATE").await;
    let mut b = client(&e, FRIEND).await;
    b.voice_video(1, LOBBY, true).await;
    b.event("VOICE_SERVER_UPDATE").await;
    a.drain().await;

    // The server's level drops (SR_ENTITLEMENT_UPDATE): limit 1.
    set_limits(&e, 1, 25);
    emit(
        &e.bus,
        DomainEvent::GuildEntitlementUpdated {
            guild: G,
            payload: json!({"type": "guild", "guild_id": "1", "level": 0}),
        },
    )
    .await;
    settle().await;
    assert!(e.gw.voice_state(u(MEMBER)).unwrap().self_video);
    assert!(e.gw.voice_state(u(FRIEND)).unwrap().self_video);
    let upd = a.event("SR_VOICE_LIMITS_UPDATE").await;
    assert_eq!(upd["video_senders"], 1);
    assert_eq!(upd["guild_id"], "1");

    // A new sender is blocked.
    let mut m = client(&e, MOD).await;
    m.voice_video(1, LOBBY, true).await;
    assert_eq!(error_code(&mut m).await["code"], "video_limit_reached");
}

// ---- Go Live streams (VOX-SIG-010) -------------------------------------------------------------

impl Client {
    async fn voice_video(&self, guild: u64, channel: ChannelId, video: bool) {
        self.send(json!({"op": 4, "d": {
            "guild_id": guild.to_string(),
            "channel_id": channel.get().to_string(),
            "self_mute": false, "self_deaf": false, "self_video": video,
        }}))
        .await;
    }

    async fn stream_start(&self, channel: ChannelId, quality: Option<(u16, u8)>) {
        let mut d = json!({"guild_id": "1", "channel_id": channel.get().to_string()});
        if let Some((h, f)) = quality {
            d["height"] = json!(h);
            d["fps"] = json!(f);
        }
        self.send(json!({"op": 18, "d": d})).await;
    }

    async fn stream_op(&self, op: u8, key: &str) {
        self.send(json!({"op": op, "d": {"stream_key": key}})).await;
    }
}

const KEY: &str = "guild:1:20:1";

#[tokio::test]
async fn a_stream_is_created_watched_and_deleted() {
    let e = env();
    let mut owner = join(&e, MEMBER, LOBBY).await;
    let mut viewer = join(&e, FRIEND, LOBBY).await;
    owner.drain().await;

    owner.stream_start(LOBBY, None).await;
    let created = owner.event("STREAM_CREATE").await;
    assert_eq!(created["stream_key"], KEY);
    assert_eq!(created["viewer_ids"], json!([]));
    assert_eq!(created["paused"], false);
    assert!(e.gw.voice_state(u(MEMBER)).unwrap().self_stream);
    // Everyone in the guild sees the LIVE state on the voice state.
    let st = viewer.event("VOICE_STATE_UPDATE").await;
    assert_eq!(st["self_stream"], true);
    assert_eq!(e.gw.stream_keys(LOBBY), vec![KEY.to_string()]);

    // Watching: the viewer gets the stream, the owner a viewer list.
    viewer.stream_op(20, KEY).await;
    assert_eq!(viewer.event("STREAM_CREATE").await["stream_key"], KEY);
    assert_eq!(
        owner.event("STREAM_UPDATE").await["viewer_ids"],
        json!(["2"])
    );
    assert_eq!(e.gw.stream_viewers(KEY), vec![u(FRIEND)]);

    // Pause is owner only.
    owner
        .send(json!({"op": 22, "d": {"stream_key": KEY, "paused": true}}))
        .await;
    assert_eq!(owner.event("STREAM_UPDATE").await["paused"], true);
    viewer
        .send(json!({"op": 22, "d": {"stream_key": KEY, "paused": false}}))
        .await;
    assert_eq!(error_code(&mut viewer).await["code"], "unknown_stream");

    // The viewer stops watching (op 19 by a non-owner).
    viewer.stream_op(19, KEY).await;
    assert_eq!(viewer.event("STREAM_DELETE").await["stream_key"], KEY);
    assert!(e.gw.stream_viewers(KEY).is_empty());
    owner.drain().await;

    // The owner stops: both are told, the state loses the flag.
    viewer.stream_op(20, KEY).await;
    viewer.event("STREAM_CREATE").await;
    owner.stream_op(19, KEY).await;
    assert_eq!(
        owner.event("STREAM_DELETE").await["reason"],
        "user_requested"
    );
    assert_eq!(viewer.event("STREAM_DELETE").await["stream_key"], KEY);
    assert!(!e.gw.voice_state(u(MEMBER)).unwrap().self_stream);
    assert!(e.gw.stream_keys(LOBBY).is_empty());
}

#[tokio::test]
async fn streams_enforce_quality_senders_viewers_and_the_room() {
    let e = env();
    set_limits(&e, 2, 1);
    let mut owner = join(&e, MEMBER, LOBBY).await;
    let mut f = join(&e, FRIEND, LOBBY).await;
    let mut m = join(&e, MOD, LOBBY).await;

    // 1080p60 is above the free tier.
    owner.stream_start(LOBBY, Some((1080, 60))).await;
    let err = error_code(&mut owner).await;
    assert_eq!(err["code"], "stream_quality_not_allowed");
    assert_eq!(err["allowed"][0]["height"], 720);
    assert!(!e.gw.voice_state(u(MEMBER)).unwrap().self_stream);

    owner.stream_start(LOBBY, Some((720, 30))).await;
    owner.event("STREAM_CREATE").await;
    // Twice is refused.
    owner.stream_start(LOBBY, None).await;
    assert_eq!(error_code(&mut owner).await["code"], "already_streaming");

    // One viewer fits, the next sees "This stream is full."
    f.stream_op(20, KEY).await;
    f.event("STREAM_CREATE").await;
    m.stream_op(20, KEY).await;
    let full = error_code(&mut m).await;
    assert_eq!(full["code"], "stream_full");
    assert_eq!(full["limit"], 1);

    // Unknown keys and the owner watching themselves.
    m.stream_op(20, "guild:1:20:99").await;
    assert_eq!(error_code(&mut m).await["code"], "unknown_stream");
    owner.stream_op(20, KEY).await;
    assert_eq!(error_code(&mut owner).await["code"], "bad_request");

    // A camera and a stream are two senders: with limit 2 and the stream out, FRIEND's
    // camera fits, MOD's is the third send.
    f.voice_video(1, LOBBY, true).await;
    f.drain().await;
    assert!(e.gw.voice_state(u(FRIEND)).unwrap().self_video);
    m.stream_start(LOBBY, None).await;
    let err = error_code(&mut m).await;
    assert_eq!(err["code"], "video_limit_reached");
    assert_eq!(err["limit"], 2);
}

#[tokio::test]
async fn leaving_ends_your_stream_and_stops_the_ones_you_watch() {
    let e = env();
    let mut owner = join(&e, MEMBER, LOBBY).await;
    let mut viewer = join(&e, FRIEND, LOBBY).await;
    owner.stream_start(LOBBY, None).await;
    owner.event("STREAM_CREATE").await;
    viewer.stream_op(20, KEY).await;
    viewer.event("STREAM_CREATE").await;
    owner.drain().await;

    // The viewer disconnects: the owner's viewer list shrinks.
    viewer.voice(1, None, false, false).await;
    assert_eq!(owner.event("STREAM_UPDATE").await["viewer_ids"], json!([]));

    viewer.voice(1, Some(LOBBY.get()), false, false).await;
    viewer.event("VOICE_SERVER_UPDATE").await;
    viewer.stream_op(20, KEY).await;
    viewer.event("STREAM_CREATE").await;
    viewer.drain().await;
    // The owner disconnects: the stream ends for the viewer.
    owner.voice(1, None, false, false).await;
    assert_eq!(viewer.event("STREAM_DELETE").await["stream_key"], KEY);
    assert!(e.gw.stream_keys(LOBBY).is_empty());

    // Moving rooms ends it too.
    let mut again = join(&e, MEMBER, LOBBY).await;
    again.stream_start(LOBBY, None).await;
    again.event("STREAM_CREATE").await;
    let moved =
        e.gw.moderate_voice(
            u(MOD),
            G,
            u(MEMBER),
            ModPatch {
                channel: Some(Some(ROOM_B)),
                ..ModPatch::default()
            },
        )
        .await;
    assert!(moved.is_ok());
    assert!(e.gw.stream_keys(LOBBY).is_empty());
    assert!(!e.gw.voice_state(u(MEMBER)).unwrap().self_stream);
}

#[tokio::test]
async fn streaming_needs_the_stream_permission_and_a_voice_connection() {
    let e = env();
    e.dir
        .perms
        .lock()
        .unwrap()
        .insert((u(FRIEND), LOBBY), member_perms() - Permissions::STREAM);
    let mut nobody = client(&e, MEMBER).await;
    nobody.stream_start(LOBBY, None).await;
    assert_eq!(error_code(&mut nobody).await["code"], "not_in_voice");
    let mut f = join(&e, FRIEND, LOBBY).await;
    f.stream_start(LOBBY, None).await;
    assert_eq!(error_code(&mut f).await["code"], "missing_permission");
    f.send(json!({"op": 18, "d": {"nonsense": true}})).await;
    assert_eq!(error_code(&mut f).await["code"], "bad_request");
}

#[tokio::test(start_paused = true)]
async fn the_streamer_uploads_a_preview_others_can_fetch() {
    let e = env();
    let mut owner = join(&e, MEMBER, LOBBY).await;
    let _viewer = join(&e, FRIEND, LOBBY).await;
    owner.stream_start(LOBBY, None).await;
    owner.event("STREAM_CREATE").await;

    let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 4];
    assert_eq!(
        e.gw.stream_preview_put(u(FRIEND), KEY, jpeg.clone()),
        Err(PreviewError::NotFound),
        "only the streamer uploads"
    );
    assert_eq!(
        e.gw.stream_preview_put(u(MEMBER), KEY, vec![0x89, b'P', b'N', b'G', 1, 2]),
        Err(PreviewError::NotJpeg)
    );
    assert_eq!(
        e.gw.stream_preview_put(u(MEMBER), KEY, vec![0xFF; 300 * 1024]),
        Err(PreviewError::TooLarge)
    );
    assert_eq!(
        e.gw.stream_preview_put(u(MEMBER), "guild:1:20:77", jpeg.clone()),
        Err(PreviewError::NotFound)
    );
    let at =
        e.gw.stream_preview_put(u(MEMBER), KEY, jpeg.clone())
            .unwrap();
    assert!(at > 0);
    // Too soon for another one, fine after a few seconds.
    assert_eq!(
        e.gw.stream_preview_put(u(MEMBER), KEY, jpeg.clone()),
        Err(PreviewError::RateLimited)
    );
    tokio::time::advance(Duration::from_secs(6)).await;
    let newer = vec![0xFF, 0xD8, 0xFF, 0xDB, 9, 9, 9];
    e.gw.stream_preview_put(u(MEMBER), KEY, newer.clone())
        .unwrap();

    let (bytes, _) = e.gw.stream_preview_get(u(FRIEND), KEY).await.unwrap();
    assert_eq!(*bytes, newer);
    // Members who cannot see the channel, and strangers, get nothing.
    assert_eq!(
        e.gw.stream_preview_get(u(OUTSIDER), KEY).await.err(),
        Some(PreviewError::NotFound)
    );
    e.dir
        .perms
        .lock()
        .unwrap()
        .insert((u(FRIEND), LOBBY), Permissions::empty());
    assert_eq!(
        e.gw.stream_preview_get(u(FRIEND), KEY).await.err(),
        Some(PreviewError::NotFound)
    );
    // Gone with the stream.
    owner.stream_op(19, KEY).await;
    owner.event("STREAM_DELETE").await;
    assert_eq!(
        e.gw.stream_preview_get(u(MEMBER), KEY).await.err(),
        Some(PreviewError::NotFound)
    );
}

// ---- stage channels (VOX-STG-001) --------------------------------------------------------------

fn stage_patch(suppress: bool) -> ModPatch {
    ModPatch {
        suppress: Some(suppress),
        ..ModPatch::default()
    }
}

#[tokio::test]
async fn a_stage_runs_from_start_to_end() {
    let e = env();
    let mut watcher = client(&e, FRIEND).await;

    // Only moderators start it, and only in stage channels.
    assert_eq!(
        e.gw.stage_create(u(MEMBER), STAGE, "Town hall", None).await,
        Err(StageError::MissingPermissions)
    );
    assert_eq!(
        e.gw.stage_create(u(MOD), LOBBY, "Town hall", None).await,
        Err(StageError::NotStageChannel)
    );
    assert_eq!(
        e.gw.stage_create(u(MOD), ChannelId::new(999), "x", None)
            .await,
        Err(StageError::UnknownChannel)
    );
    assert_eq!(
        e.gw.stage_create(u(MOD), STAGE, "  ", None).await,
        Err(StageError::InvalidTopic)
    );
    assert_eq!(
        e.gw.stage_create(u(MOD), STAGE, "ok", Some(1)).await,
        Err(StageError::InvalidPrivacy)
    );
    let inst =
        e.gw.stage_create(u(MOD), STAGE, " Town hall ", None)
            .await
            .unwrap();
    assert_eq!(inst.topic, "Town hall");
    let ev = watcher.event("STAGE_INSTANCE_CREATE").await;
    assert_eq!(ev["channel_id"], "24");
    assert_eq!(ev["topic"], "Town hall");
    assert_eq!(ev["privacy_level"], 2);
    assert_eq!(
        e.gw.stage_create(u(MOD), STAGE, "again", None).await,
        Err(StageError::AlreadyExists)
    );

    // Update.
    let upd =
        e.gw.stage_update(u(MOD), STAGE, Some("Q and A"), None)
            .await
            .unwrap();
    assert_eq!(upd.topic, "Q and A");
    assert_eq!(
        watcher.event("STAGE_INSTANCE_UPDATE").await["topic"],
        "Q and A"
    );
    assert_eq!(
        e.gw.stage_update(u(MEMBER), STAGE, Some("x"), None).await,
        Err(StageError::MissingPermissions)
    );
    assert_eq!(e.gw.stage_instance(STAGE).unwrap().topic, "Q and A");

    // Listing respects visibility.
    assert_eq!(e.gw.stage_instances_for(u(FRIEND), G).await.len(), 1);

    // Ending needs a moderator and a running stage.
    assert_eq!(
        e.gw.stage_end(u(MEMBER), STAGE).await,
        Err(StageError::MissingPermissions)
    );
    e.gw.stage_end(u(MOD), STAGE).await.unwrap();
    assert_eq!(
        watcher.event("STAGE_INSTANCE_DELETE").await["channel_id"],
        "24"
    );
    assert_eq!(
        e.gw.stage_end(u(MOD), STAGE).await,
        Err(StageError::NoInstance)
    );
    assert!(e.gw.stage_instance(STAGE).is_none());
}

#[tokio::test]
async fn the_audience_listens_until_a_moderator_invites_them_to_speak() {
    let e = env();
    e.gw.stage_create(u(MOD), STAGE, "Panel", None)
        .await
        .unwrap();
    let mut m = client(&e, MOD).await;
    m.voice(1, Some(STAGE.get()), false, false).await;
    let srv = m.event("VOICE_SERVER_UPDATE").await;
    let mod_claims = TokenSigner::new(SECRET.to_vec())
        .verify(srv["token"].as_str().unwrap(), unix_now())
        .unwrap();
    assert!(mod_claims.speak, "moderators speak at once");
    assert!(!e.gw.voice_state(u(MOD)).unwrap().suppress);

    let mut a = client(&e, MEMBER).await;
    a.voice(1, Some(STAGE.get()), false, false).await;
    let srv = a.event("VOICE_SERVER_UPDATE").await;
    let claims = TokenSigner::new(SECRET.to_vec())
        .verify(srv["token"].as_str().unwrap(), unix_now())
        .unwrap();
    assert!(!claims.speak, "the audience is muted at the SFU");
    let st = e.gw.voice_state(u(MEMBER)).unwrap();
    assert!(st.suppress && st.request_to_speak_ms.is_none());
    m.drain().await;
    e.relay.take();

    // Raising a hand shows a timestamp to the moderator.
    let st =
        e.gw.stage_self_update(
            u(MEMBER),
            G,
            STAGE,
            SelfStagePatch {
                request_to_speak: Some(true),
                ..SelfStagePatch::default()
            },
        )
        .await
        .unwrap();
    assert!(st.request_to_speak_ms.is_some());
    let ev = m.event("VOICE_STATE_UPDATE").await;
    assert_eq!(ev["user_id"], "1");
    assert!(ev["request_to_speak_timestamp"].is_string());
    assert_eq!(ev["suppress"], true);

    // The audience cannot make themselves a speaker.
    assert_eq!(
        e.gw.stage_self_update(
            u(MEMBER),
            G,
            STAGE,
            SelfStagePatch {
                suppress: Some(false),
                ..SelfStagePatch::default()
            }
        )
        .await,
        Err(StageError::MissingPermissions)
    );
    // And a member without Request to Speak cannot raise a hand.
    e.dir.perms.lock().unwrap().insert(
        (u(FRIEND), STAGE),
        member_perms() - Permissions::REQUEST_TO_SPEAK,
    );
    let _f = join(&e, FRIEND, STAGE).await;
    assert_eq!(
        e.gw.stage_self_update(
            u(FRIEND),
            G,
            STAGE,
            SelfStagePatch {
                request_to_speak: Some(true),
                ..SelfStagePatch::default()
            }
        )
        .await,
        Err(StageError::MissingPermissions)
    );

    // Invite to speak: the flags reach the SFU and the hand comes down.
    e.relay.take();
    let out =
        e.gw.moderate_voice(u(MOD), G, u(MEMBER), stage_patch(false))
            .await
            .unwrap();
    assert_eq!(out.actions, vec![ModAction::Suppressed(false)]);
    let st = e.gw.voice_state(u(MEMBER)).unwrap();
    assert!(!st.suppress && st.request_to_speak_ms.is_none());
    assert!(matches!(
        e.relay.take()[..],
        [VoiceCommand::SetFlags { speak: true, .. }]
    ));

    // A speaker may step down on their own, and a moderator may send them back.
    e.gw.stage_self_update(
        u(MEMBER),
        G,
        STAGE,
        SelfStagePatch {
            suppress: Some(true),
            ..SelfStagePatch::default()
        },
    )
    .await
    .unwrap();
    assert!(e.gw.voice_state(u(MEMBER)).unwrap().suppress);
    e.gw.moderate_voice(u(MOD), G, u(MEMBER), stage_patch(false))
        .await
        .unwrap();
    let out =
        e.gw.moderate_voice(u(MOD), G, u(MEMBER), stage_patch(true))
            .await
            .unwrap();
    assert_eq!(out.actions, vec![ModAction::Suppressed(true)]);
    assert!(matches!(
        e.relay.take().last(),
        Some(VoiceCommand::SetFlags { speak: false, .. })
    ));
}

#[tokio::test]
async fn stage_rules_reject_the_wrong_channel_and_the_wrong_person() {
    let e = env();
    let mut a = join(&e, MEMBER, LOBBY).await;
    let _ = &mut a;
    // Suppress only means something in a stage channel.
    assert_eq!(
        e.gw.moderate_voice(u(MOD), G, u(MEMBER), stage_patch(true))
            .await,
        Err(ModError::NotStageChannel)
    );
    // Self updates need the stage the member is in.
    assert_eq!(
        e.gw.stage_self_update(u(MEMBER), G, STAGE, SelfStagePatch::default())
            .await,
        Err(StageError::NotInVoice)
    );
    assert_eq!(
        e.gw.stage_self_update(u(MEMBER), G, LOBBY, SelfStagePatch::default())
            .await,
        Err(StageError::NotStageChannel)
    );
    // No hand can go up without a running stage.
    let _s = join(&e, FRIEND, STAGE).await;
    assert_eq!(
        e.gw.stage_self_update(
            u(FRIEND),
            G,
            STAGE,
            SelfStagePatch {
                request_to_speak: Some(true),
                ..SelfStagePatch::default()
            }
        )
        .await,
        Err(StageError::NoInstance)
    );
    // A moderator moving someone into a stage puts them in the audience.
    let out =
        e.gw.moderate_voice(
            u(MOD),
            G,
            u(MEMBER),
            ModPatch {
                channel: Some(Some(STAGE)),
                ..ModPatch::default()
            },
        )
        .await
        .unwrap();
    assert!(out.state.unwrap().suppress);
}

#[tokio::test]
async fn ending_a_stage_sends_speakers_back_and_an_empty_stage_ends_itself() {
    let e = env();
    e.gw.stage_create(u(MOD), STAGE, "Show", None)
        .await
        .unwrap();
    let mut m = join(&e, MOD, STAGE).await;
    let _a = join(&e, MEMBER, STAGE).await;
    e.gw.moderate_voice(u(MOD), G, u(MEMBER), stage_patch(false))
        .await
        .unwrap();
    assert!(!e.gw.voice_state(u(MEMBER)).unwrap().suppress);
    m.drain().await;

    e.gw.stage_end(u(MOD), STAGE).await.unwrap();
    assert!(e.gw.voice_state(u(MEMBER)).unwrap().suppress);
    assert!(
        !e.gw.voice_state(u(MOD)).unwrap().suppress,
        "moderators keep the floor"
    );

    // The next stage ends when the last member leaves.
    e.gw.stage_create(u(MOD), STAGE, "Second", None)
        .await
        .unwrap();
    m.drain().await;
    e.gw.voice_leave(u(MEMBER)).await;
    assert!(e.gw.stage_instance(STAGE).is_some());
    e.gw.voice_leave(u(MOD)).await;
    assert!(e.gw.stage_instance(STAGE).is_none());
}

#[tokio::test]
async fn deleting_a_stage_channel_ends_its_stage() {
    let e = env();
    // A connected session makes the gateway follow the guild's events.
    let _c = client(&e, MOD).await;
    e.gw.stage_create(u(MOD), STAGE, "Doomed", None)
        .await
        .unwrap();
    emit(
        &e.bus,
        DomainEvent::ChannelDeleted {
            guild: G,
            channel: STAGE,
            payload: json!({"id": "24"}),
            viewers: vec![u(MOD)],
        },
    )
    .await;
    settle().await;
    assert!(e.gw.stage_instance(STAGE).is_none());
}

// ---- DM calls (VOX-SIG-008) ---------------------------------------------------------------------

impl Client {
    async fn call(&self) {
        self.send(json!({"op": 4, "d": {
            "guild_id": null, "channel_id": DM.get().to_string(),
            "self_mute": false, "self_deaf": false,
        }}))
        .await;
    }
}

#[tokio::test]
async fn a_call_rings_updates_and_ends_with_a_record() {
    let e = env();
    let mut caller = client(&e, MEMBER).await;
    let mut friend = client(&e, FRIEND).await;
    let mut third = client(&e, MOD).await;
    let mut stranger = client(&e, OUTSIDER).await;

    caller.call().await;
    // The state and the call are announced before the token arrives.
    caller.event("CALL_CREATE").await;
    let srv = caller.event("VOICE_SERVER_UPDATE").await;
    assert_eq!(srv["channel_id"], "300");
    assert!(srv["guild_id"].is_null());
    let claims = TokenSigner::new(SECRET.to_vec())
        .verify(srv["token"].as_str().unwrap(), unix_now())
        .unwrap();
    assert_eq!(claims.guild_id, None);
    assert!(claims.speak);

    // Voice state updates reach the DM members and nobody else.
    assert_eq!(
        friend.event("VOICE_STATE_UPDATE").await["channel_id"],
        "300"
    );
    // Everyone else in the DM is rung.
    let create = friend.event("CALL_CREATE").await;
    assert_eq!(create["channel_id"], "300");
    assert_eq!(create["ringing"], json!(["2", "4"]));
    assert_eq!(create["voice_states"].as_array().unwrap().len(), 1);
    assert_eq!(
        third.event("CALL_CREATE").await["ringing"],
        json!(["2", "4"])
    );
    assert!(
        stranger
            .drain()
            .await
            .iter()
            .all(|v| v["t"] != "VOICE_STATE_UPDATE" && v["t"] != "CALL_CREATE")
    );

    // Answering stops that member's ring.
    friend.call().await;
    friend.event("VOICE_SERVER_UPDATE").await;
    assert_eq!(third.event("CALL_UPDATE").await["ringing"], json!(["4"]));

    // Declining stops the other one.
    e.gw.call_stop_ringing(u(MOD), DM, None).await.unwrap();
    // The caller saw FRIEND answer, then MOD decline.
    assert_eq!(caller.event("CALL_UPDATE").await["ringing"], json!(["4"]));
    assert_eq!(caller.event("CALL_UPDATE").await["ringing"], json!([]));
    third.drain().await;
    assert!(e.gw.call_state(u(MEMBER), DM).is_some());
    assert!(e.gw.call_state(u(OUTSIDER), DM).is_none());

    // Members outside the call cannot ring; members inside can ring again.
    assert_eq!(
        e.gw.call_ring(u(MOD), DM, None).await,
        Err(CallError::NotInCall)
    );
    assert_eq!(
        e.gw.call_ring(u(MEMBER), DM, Some(vec![u(OUTSIDER)])).await,
        Err(CallError::NotAMember)
    );
    e.gw.call_ring(u(MEMBER), DM, Some(vec![u(MOD)]))
        .await
        .unwrap();
    assert_eq!(third.event("CALL_UPDATE").await["ringing"], json!(["4"]));
    // Another member cannot stop a ring for someone else from outside the call.
    assert_eq!(
        e.gw.call_stop_ringing(u(MOD), DM, Some(vec![u(FRIEND)]))
            .await,
        Err(CallError::NotInCall)
    );

    // The call ends when the last member leaves.
    caller
        .send(json!({"op": 4, "d": {"guild_id": null, "channel_id": null}}))
        .await;
    friend
        .send(json!({"op": 4, "d": {"guild_id": null, "channel_id": null}}))
        .await;
    assert_eq!(third.event("CALL_DELETE").await["channel_id"], "300");
    settle().await;
    let log = e.calls.0.lock().unwrap().clone();
    assert_eq!(log.len(), 1, "{log:?}");
    assert_eq!(log[0].channel_id, DM);
    assert_eq!(log[0].initiator, u(MEMBER));
    assert_eq!(log[0].participants, vec![u(MEMBER), u(FRIEND)]);
    assert_eq!(log[0].missed_by, vec![u(MOD)], "MOD never joined");
    assert!(log[0].ended_ms >= log[0].started_ms);
    assert!(e.gw.call_snapshot().is_empty());
}

#[tokio::test(start_paused = true)]
async fn an_unanswered_call_stops_ringing_after_the_timeout() {
    let e = env();
    let mut caller = client(&e, MEMBER).await;
    let mut friend = client(&e, FRIEND).await;
    caller.call().await;
    caller.event("VOICE_SERVER_UPDATE").await;
    assert_eq!(
        friend.event("CALL_CREATE").await["ringing"],
        json!(["2", "4"])
    );

    tokio::time::advance(RING_TIMEOUT - Duration::from_secs(1)).await;
    e.gw.sweep().await;
    assert_eq!(e.gw.call_snapshot()[&DM].1, vec![u(FRIEND), u(MOD)]);
    tokio::time::advance(Duration::from_secs(2)).await;
    e.gw.sweep().await;
    assert_eq!(friend.event("CALL_UPDATE").await["ringing"], json!([]));
    assert!(e.gw.call_snapshot()[&DM].1.is_empty());

    // The caller gives up: the call is deleted and everyone who never joined missed it.
    caller
        .send(json!({"op": 4, "d": {"guild_id": null, "channel_id": null}}))
        .await;
    assert_eq!(friend.event("CALL_DELETE").await["channel_id"], "300");
    let log = e.calls.0.lock().unwrap().clone();
    assert_eq!(log[0].participants, vec![u(MEMBER)]);
    assert_eq!(log[0].missed_by, vec![u(FRIEND), u(MOD)]);
}

#[tokio::test]
async fn only_members_of_the_dm_can_join_its_call() {
    let e = env();
    let mut outsider = client(&e, OUTSIDER).await;
    outsider.call().await;
    assert_eq!(error_code(&mut outsider).await["code"], "not_member");
    let mut no_dm = client(&e, MUTED_OUT).await;
    no_dm.call().await;
    assert_eq!(error_code(&mut no_dm).await["code"], "not_member");
    assert!(
        e.gw.call_snapshot().is_empty(),
        "refused joins leave nothing behind"
    );
}

#[tokio::test]
async fn video_in_a_call_uses_the_free_allowance() {
    let e = env();
    set_limits(&e, 1, 5);
    let mut a = client(&e, MEMBER).await;
    a.send(json!({"op": 4, "d": {"guild_id": null, "channel_id": "300", "self_video": true}}))
        .await;
    a.event("VOICE_SERVER_UPDATE").await;
    assert!(e.gw.voice_state(u(MEMBER)).unwrap().self_video);
    let mut b = client(&e, FRIEND).await;
    b.send(json!({"op": 4, "d": {"guild_id": null, "channel_id": "300", "self_video": true}}))
        .await;
    assert_eq!(error_code(&mut b).await["code"], "video_limit_reached");
}

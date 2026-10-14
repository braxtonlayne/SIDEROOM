# Handoff: feat/p10-voice-fe (phase 10, voice and video front end)

Branch `feat/p10-voice-fe`, based on `integration/phase10`. Web only (`apps/web`), plus 8 icons in `packages/ui`
and inventory notes. No backend changes, no new routes, nothing pushed. Inventory: `docs/inventory/08-lounge-voice.md`
(`VOX-E2E-*` left to others).

## What the audit found

The audio call was built in wave 5 (`feat/p5-voice-client`). The SFU forwards **audio only**: the voice spec says
video m-lines "would be accepted and ignored", the VIDEO (12) and QUALITY (13) ops are reserved, there is no
soundboard, stage, DM call or stream-key backend (VOX-SFU-005/006/008, VOX-SIG-008/009/010, VOX-STG-001, VOX-SB-001).
So everything below is built against what exists (REST entitlement routes, voice-state flags, message routes for
voice channels, `PATCH /guilds/{g}/voice-states/{u}`) or against a small client-side contract written down next to
the code, and video is a local preview until the SFU forwards it.

## Done

- **VOX-CLI-001** (ticked). A device lost mid-call no longer ends the call: `controller.recoverMic` reopens the
  system default input through `engine.setInputDevice`, shows a dismissible note, and only ends the call (old fix-it
  message) if the default fails too or the device keeps vanishing (3 times per call).
- **VOX-UI-001** (partial). Ping graph (sparkline of the last 30 readings plus average and peak) in the quality
  tooltip, camera and screen share buttons, recovered-mic note, refusal and governor notices.
- **VOX-UI-002** (ticked). Tile grid, focus mode (spotlight plus a strip of the other tiles), full screen, Hide
  non-video participants, and a camera preview on the Join Voice screen (`JoinPreview`).
- **VOX-UI-003 / 004** (partial). Participant tile shows the camera when there is one (own camera, remote through
  the `useRemoteVideo` store), state icons, own-tile quality bars. Stream tile: LIVE badge, Watch Stream / Stop
  Watching, volume, full screen, quality info; your own share shows live with a Stop button.
- **VOX-UI-005** (partial). Bottom controls: mic with an input device menu, deafen, camera with a camera menu, share
  screen with options (change quality, content hint, stop), text chat, watch party, More (layout, hide non-video, full
  screen, pop-out) and disconnect.
- **VOX-UI-006** (ticked) + **VOX-CLI-006/007** (partial). The share dialog (source type, resolution and fps radios,
  share audio, motion/detail hint). Qualities above the viewer's entitlement are disabled with a "Needs Patron or a
  higher server level" hint. Entitlements are real: `GET /donations/me` limits merged with
  `GET /guilds/{id}/support` perks (`voice/limits.ts`, same "fits one profile" rule as `Limits::allows_video`).
  `voice/video.ts` has the capture code (`getUserMedia` camera clamped to the best allowed profile with fallback to
  the default camera, `getDisplayMedia` with audio, `contentHint`, "Stop sharing" handling) and the f/h/q simulcast
  encodings helper.
- **VOX-UI-007** (ticked). Text-in-voice: the voice channel's message list and composer in a panel beside the call
  (voice channels accept messages in `ChannelType::accepts_messages`).
- **VOX-UI-008** (partial). One menu (`useVoiceUserMenu`) in tiles and the channel list: Profile, Mention, Message,
  Add Note, User Volume, Mute, Server Mute/Deafen, Move To, Disconnect, Timeout, Kick, Ban, Copy User ID.
- **VOX-UI-009** (ticked). Drag a person (when you hold MOVE_MEMBERS) onto another voice channel in the same server;
  the drop calls the moderator route and the target highlights.
- **VOX-UI-010** (partial). Incoming call overlay (avatar, Accept, Accept with video, Decline, repeating synthesized
  ringtone, 30 s timeout) driven by `CALL_CREATE/UPDATE/DELETE`, mounted at the app root.
- **VOX-UI-012** (ticked). Pop-out of the call into a Document Picture-in-Picture window (only offered where the API
  exists), styles copied, closes with the call.
- **VOX-UI-013 / 014** (ticked, client side). `SR_VOICE_ERROR` `video_limit` / `viewer_cap` keep the call, stop the
  camera and show the limit with Support / Patron links; QUALITY op `{reduced}` shows "Reduced quality: the room is
  busy".
- **VOX-UI-016** (partial). Compact tiles, swipe down on the call to minimise (when the lounge gives a back
  handler, i.e. mobile), one scrolling 44 px controls bar.

## Still open, with reasons

| Item | Why |
| --- | --- |
| VOX-CLI-006/007 publish half | The SFU forwards no video (VOX-SFU-005/006). `VIDEO_PUBLISH` in `voice/protocol.ts` stays `false`: no video m-lines, no `self_video`/`self_stream` announced (so nobody sees a LIVE tile with nothing in it). The share dialog says it is a preview. With the flag on the controller already sends `self_video`/`self_stream` (tested). |
| VOX-CLI-008 | Needs the preview upload endpoint (VOX-SIG-010). |
| VOX-UI-001 Activities, soundboard buttons; VOX-UI-005 same | Activities is a phase-6 launcher; no soundboard backend. |
| VOX-UI-003 remote quality, E2EE shield | No remote stats; E2EE is VOX-E2E-007. |
| VOX-UI-004 viewer count, preview, stream pop-out | VOX-SIG-010 and remote video. |
| VOX-UI-008 Call, Disable Video | DM/group calls (VOX-SIG-008), remote video. |
| VOX-UI-010 outgoing ringing | There is no way to start a DM call yet: gateway op 4 without a guild is refused (`not_voice_channel`). |
| VOX-UI-011 DM call view | Same. |
| VOX-UI-015 channel status | No backend field. |
| VOX-UI-016 bottom sheet | The controls are a scrolling bar, not a sheet. |
| VOX-STG-002 | Stage instances do not exist (VOX-STG-001); join to a stage is refused. |
| VOX-SB-002/003 | No guild sounds API or send-sound event (VOX-SB-001). |

## Backend gaps to close (contracts the client already speaks)

- `CALL_CREATE {channel_id, initiator_id?, ringing[], video?, group?}`, `CALL_UPDATE {channel_id, ringing[]}`,
  `CALL_DELETE {channel_id}` dispatches; allow op 4 for DM channels; a stop-ringing route for Decline (today Decline
  is local only).
- `SR_VOICE_ERROR` codes `video_limit` and `viewer_cap`, each with an optional numeric `limit`.
- Voice WS op 13 `QUALITY {reduced: bool}` from the egress governor.
- Video forwarding (op 12 and video m-lines), then flip `VIDEO_PUBLISH`, feed `setRemoteVideo()` from the engine, and
  use `simulcastEncodings()` in `addTransceiver`.

## Tests run

- `apps/web` vitest, voice and lounge voice suites: `voice/__tests__/{video,controller,engine}` (capture, caps,
  encodings, recovery, ping history, refusals, governor), `lounge/voice/__tests__/{callUi,incomingCall,popOutSwipe,voiceUi}`
  (picker gating, controls, camera and share through fake `getUserMedia`/`getDisplayMedia`, menus, drag, overlay,
  notices, pop-out, swipe, axe checks). Media APIs are the existing doubles plus small stream fakes; no real devices.
- Whole web suite (111 files): all pass; 7 unrelated files (admin cinema, lounge messages/shell/serverFlows, account) time out when run with 3 workers on the shared machine and pass when rerun with 2. `tsc --noEmit`, eslint, prettier `--check` clean for `apps/web/src` and `packages/ui/src`; `packages/ui` icon test passes; `vite build` ok and `scripts/check-bundle-budget.mjs` ok (lounge 312 KB gzip of 600).

## Decisions

- Entitlement source: the existing donation routes; no new endpoint. A failed request leaves the free tier (720p30).
- Video stays local until the SFU says otherwise (flag, not a guess).
- New catalog fragments `p10-voice-fe-1.ts`, `p10-voice-fe-2.ts`; new styles in `lounge/voice/voice-p10.css` (logical
  properties only). Icons added to `packages/ui`: video-off, screen-share, maximize, minimize, message, phone, pop-out,
  soundboard (unused yet).
- Em dashes avoided in UI copy ("Reduced quality: the room is busy.").

## Open owner questions

1. Should Decline on a ringing DM call also tell the server (needs a route), or stay local?
2. Is 30 s the ring time you want, matching the future server timeout (VOX-SIG-008)?
3. Free tier camera/screen cap is read from the server perk table (720p30); confirm the presets offered (480p30 to
   1080p60) match what you want to advertise.

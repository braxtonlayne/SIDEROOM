# Handoff: feat/p5-voice-client (wave 5, voice client and UI)

Branch `feat/p5-voice-client`, based on `integration/phase4`. Frontend only (`apps/web`), plus one Vite proxy line, one e2e spec and inventory notes. Nothing is pushed.

## Done

**Gateway and store**
- `VOICE_STATE_UPDATE` (op 4) is sent by the voice controller; `VOICE_STATE_UPDATE` and `VOICE_SERVER_UPDATE` are reduced into the store (`voiceStates[guild][user]`, `voiceServer`), `GUILD_CREATE.voice_states` seeds them, `READY` clears them, `GUILD_DELETE` drops them. DM calls use the `@me` bucket. `SR_VOICE_ERROR` is turned into a message. The gateway client now asks for the `GUILD_VOICE_STATES` intent.
- Speaking flags live in the voice session store (`src/voice/session.ts`), not the reducer, because they come from the voice WebSocket.

**`apps/web/src/voice/` (the engine, no UI)**
- `protocol.ts`: the `sr-voice-proto` opcodes, close codes, `tuneOpus` (DTX, FEC, mono, bitrate), `voiceUrl`.
- `engine.ts`: `RtcVoiceEngine` behind the `VoiceEngine` interface. Voice WebSocket (IDENTIFY, READY, heartbeat with ack timeout, RESUME with the resume token), one RTCPeerConnection, client offer after ICE gathering, server renegotiation offers answered, impolite peer on glare (spec 4.4), remote streams mapped by msid stream id = user id, ICE restart (after 2 s of "disconnected", on "failed", and on the browser `online` event), retry budgets, bitrate cap via `setParameters`, stats-based quality.
- `mic.ts`: getUserMedia with echo cancellation and AGC by choice and `noiseSuppression: false` always (D-045), input gain, level meter, transmit gate, fallback to the default device, typed errors (`permission-denied`, `no-device`, `in-use`).
- `playback.ts`: one `<audio>` element plus a WebAudio gain per person (0 to 200%), local mute, master volume, deafen silences every remote element, `setSinkId` output device, priority-speaker ducking.
- `vad.ts`: energy gate with automatic sensitivity (noise-floor tracking) or a manual threshold, hang time.
- `ptt.ts` and `keybind.ts`: tab-focused push-to-talk (capture-phase listener that swallows the key so it never reaches the composer or any other handler, release delay 0 to 2000 ms, blur releases at once) and press-to-act binds (mute, deafen, disconnect). Recording a key suspends every bind.
- `controller.ts`: join/leave policy, mute and deafen (undeafen restores the old mute state, unmuting while deafened undeafens), the "transmitting" decision (VAD or PTT, forced PTT when the channel denies USE_VAD, server mute), cues, reconnect (asks the gateway for a new server by leaving then re-joining, as spec 4.5 requires), moderator disconnect and move handling, `SR_VOICE_ERROR`.
- `sounds.ts`: synthesized join/leave, mute/deafen and PTT cues (no audio files). `settings.ts`: device-local, persisted, sanitized settings. `hooks.ts`: participants, devices with hot-plug, mic permission, mic test.

**UI**
- Channel list: participants under each voice channel (speaking ring, mute/deaf icons, LIVE, camera), user-limit counter, click joins and opens the view, context menu per participant.
- Voice connection panel above the user panel (`RTC Connecting` / `Voice Connected` / `Reconnecting`, channel and server link, quality bars with ping tooltip, voice settings, disconnect, errors with the right fix for a blocked mic).
- Voice channel view: Join Voice screen, participant tile grid (avatar, speaking ring, mute/deaf icons, per-person volume popover 0 to 200%), call controls (mute, deafen, disconnect). Audio only; video and screen share are later.
- Member context menus (channel list, tiles and the member list): User Volume, local Mute, and for moderators Server Mute, Server Deafen, Move To, Disconnect (`PATCH /guilds/{g}/voice-states/{u}`).
- Settings > Voice & Video (new section `voice` in the nav): input/output pickers with permission handling and hot-plug, input/output volume 0 to 200%, mic test with a live meter, input mode (Voice Activity with automatic sensitivity or a manual threshold with a live meter and threshold marker, or Push to Talk with the keybind recorder and release delay), echo cancellation and AGC toggles (no noise suppression control), sounds with volume.
- Settings > Keybinds: the voice keybinds (PTT, toggle mute, toggle deafen, disconnect) with the recorder and a conflict check; the built-in shortcuts stay read-only.
- The call lives in a global store and `VoiceBridge` is mounted at the app root (`main.tsx`), so it survives leaving the Lounge (VOX-CLI-010).

## Tests

- `apps/web`: 113 new tests in `src/voice/__tests__` (protocol, VAD, keybinds, PTT incl. release delay and "key not sent to the composer", mic, playback incl. deafen and 200% gain, cues, engine against fake socket/peer/media, controller against a mock engine, hooks), 13 in `src/lounge/voice/__tests__` (list, view, panel, menus, moderator PATCH, axe), 15 in `src/settings/user/__tests__/voice.test.tsx` (settings pages, recorder, permissions, axe), 8 in `src/store/voice.test.ts`. No test touches a real device; vitest uses fakes only.
- Whole web suite: 776 tests pass (62 files); typecheck, eslint, prettier and vite build clean.
- Real Chromium smoke (not committed, throwaway scripts): with `--use-fake-device-for-media-stream` the Voice & Video page lists the fake devices and the meter moves; the real `RtcVoiceEngine` against an in-page fake SFU (a second real RTCPeerConnection speaking the protocol) connected, sent Opus to the SFU side, received a server-renegotiated forwarded tone (inbound audio bytes > 0 both ways), applied the tuned Opus fmtp (`usedtx=1;useinbandfec=1;stereo=0;maxaveragebitrate=64000`), survived an ICE restart (new ufrag, bytes kept growing) and reported `reconnecting` on a socket drop.
- `e2e/tests/voice-live.spec.ts`: two Chromium contexts join one voice channel with the fake-device flags and assert inbound audio RTP bytes > 0 on both sides. It is skipped unless `SR_E2E_VOICE=1` (see "Live test" below for what was and was not run).

## Contract assumptions (spec section 4 on `feat/p5-voice-sfu`, commit 7c422b2)

I first built against my own reading of the protocol and then re-aligned to the contract once it was committed (opcodes, IDENTIFY with the token only, READY/RESUME with `resume_token`, SDP_OFFER/ANSWER, server renegotiation with `a=msid` = user id, SPEAKING 1/0, close codes 4004/4006/4009/4014/4015/4016/4017, `SR_VOICE_ERROR`, leave-then-join to rejoin). Everything protocol-specific is in `voice/protocol.ts` and `voice/engine.ts`. Things I decided that the contract leaves open:
1. The voice WebSocket is started on `VOICE_SERVER_UPDATE` alone (its `channel_id` must match the target; a stale update after a move is ignored). My own `VOICE_STATE_UPDATE` is only used for mute/deafen/move/disconnect.
2. On 4014 the client closes media and waits for the gateway (a disconnect arrives as `channel_id: null`, a move as a new channel then a `VOICE_SERVER_UPDATE`); it does not re-join by itself. On 4016 (another tab) and 4017 (full) it stops with a message.
3. The client reports SPEAKING 1/0 for both VAD and PTT (the contract allows VAD to skip it).
4. Channel bitrate comes from the store channel's `bitrate` when the API provides one (the web `Channel` type gained `bitrate` and `user_limit`); otherwise the Opus cap defaults to 64 kbps.
5. Voice permissions come from the Rust permission engine once it loads (new optional `connect/speak/useVad/muteMembers/deafenMembers/moveMembers` on the Lounge `ChannelPermissions`), else the guild-level bits, else permissive defaults. The server stays the security boundary.
6. A TURN list in `VOICE_SERVER_UPDATE.ice_servers` is used as given; the client never calls `GET /sr/voice/turn`.

## Decisions

- Voice settings are device-local (localStorage `sr.voice.settings`) and not part of the synced settings blob: devices, keybinds and per-person volumes are machine specific. They apply immediately; the page has no Save bar.
- The lounge store's placeholder voice fields (`voice`, `micMuted`, `deafened`, `joinVoice`, ...) are gone; `src/voice/session.ts` is the single source of truth. The shell test that used them now installs a controller with a stub engine.
- Voice Activity is the default input mode (D-046); PTT is the user's choice, and a channel denying USE_VAD forces it (the panel and the join screen say so, and the PTT key is only swallowed while PTT is in effect and a call is live).
- No audio assets: cues are synthesized. No ML VAD (the inventory item says optional).
- The old `KeybindsSection` in `settings/user/sections/App.tsx` is left in place (now unused) to avoid colliding with the user-settings agent's edits to that file; `UserSettings.tsx` renders the new `sections/Keybinds.tsx`. Delete the old one when merging.

## Not done / partial (inventory notes are written next to each item)

- VOX-CLI-001 partial: a device lost mid-call ends the call with a fix-it message instead of recovering.
- VOX-UI-001, 002, 003, 005, 008 partial (audio-only parts built). Not built: ping graph, camera/screen share/activity/soundboard buttons, focus mode, sidebar tiles, full screen, camera preview, E2EE shield, per-tile quality, text-in-voice, drag between channels, stream tiles, mobile call UI, stage channels, DM calls and ringing, soundboard, pop-out.
- LNG-USET-046 (attenuation), 047/048 (camera, screen share defaults), 050 (advanced), the incoming-ring sound and the soundboard volume of 049, and custom navigation keybinds in 053.
- Ticked: VOX-CLI-002, 003, 004, 005, 009, 010 and LNG-USET-043, 044, 045.
- Per-participant `channel_id` hint for the context menu is passed by the channel list and tiles; the member list relies on the store's voice state.

## Live test

Ran against a backend built from `feat/p5-voice-sfu` at 7c422b2 (extracted with `git archive` into a scratch dir, built offline into the shared target dir, throwaway Postgres DB, `SR_VOICE__ENABLED=true`, dummy storage, Vite on 5173 from this worktree, all stopped and the DB dropped afterwards). `e2e/tests/voice-live.spec.ts` passed on Chromium (1 test, 8.8 s): owner creates a server and a voice channel, a second user joins by invite, both join the channel through the UI, both panels reach "Voice Connected", both lists show 2 people, and inbound audio RTP bytes are > 0 on both sides. It needs `SR_E2E_VOICE=1` (skipped otherwise) and Chromium flags `--use-fake-device-for-media-stream --use-fake-ui-for-media-stream --allow-loopback-in-peer-connection`. `SR_E2E_VOICE_DEBUG=1` prints the voice and gateway WebSocket traffic. Firefox and WebKit were not run (fake-media flags are Chromium only).

Last contract changes applied: mic transceiver is `sendonly`, client is the impolite peer on glare (ignores a server offer while its own is outstanding, no rollback), SDP work is serialized in order, close 4008 is handled as a reconnect-and-RESUME, rejoin is leave-then-join, SPEAKING 1 on key down and 0 after the release delay, `/voice` ws proxy in `apps/web/vite.config.ts`. Remaining assumptions are the numbered list above.

## Merge notes

- Shared files touched additively: `store/reducer.ts`, `store/types.ts` (voice state, `Channel.user_limit/bitrate`), `gateway/*` (intent), `lounge/data.ts`, `lounge/liveMap.ts` (PERM bits and channel voice permissions), `lounge/ChannelSidebar.tsx`, `ChannelView.tsx`, `MemberList.tsx`, `UserPanel.tsx`, `Chrome.tsx`, `GuildRail.tsx`, `lounge.css`, `settings/user/nav.ts` (new `voice` section), `UserSettings.tsx`, `user-settings.css`, `main.tsx`, `vite.config.ts`.
- `lounge/store.ts` lost the placeholder voice fields.
- `docs/inventory/08-lounge-voice.md` is also edited on `feat/p5-voice-sfu`; resolve the tick conflicts with the union script.

# Spec — Watch Parties

Phase 5 · Decisions: D-110 · Permissions: `START_WATCH_PARTY`, `CONTROL_WATCH_PARTY` (doc 07).

## 1. What it is
Synchronized playback of Cinema content (VOD or a Live TV channel) for a group, with chat and voice. Each viewer streams the HLS media themselves from SIDE ROOM. Only **control state** is synchronized, so a party costs about the same as N people watching separately.

## 2. Where parties live
| Entry point | Result |
|---|---|
| Title page → **Watch together** | Choose: a Lounge text/voice channel you can post in, a DM/group DM, or "Private party" (shareable link to friends) (the picker lists Watch party rooms first and preselects the last destination used in the server, D-264) |
| Lounge voice channel → **Start Watch Party** (call bar button) | Party bound to that voice channel. Voice continues as normal (E2EE call) |
| **Watch party room** preset (create-channel dialog, D-208) | There is no channel type of its own: `POST /guilds/{id}/channels {type: 2, sr_watch_party_room: true, sr_watch_party_title_id?, sr_watch_party_episode_id?}` (the title is optional since D-227: without one the room starts empty, `media: null`) creates the voice channel and a persistent party bound to it in one request (the new channel is removed again when the party cannot start). The creator is not left in the party; the first person to join the empty room becomes host |
| Lounge message `<cinema:…>` card → **Watch together** | Starts a party in that channel |
| Live TV → **Watch together** (guide now panel, live player) | Party on a live channel: `POST /watch-parties {tv_channel_id}` (instead of `title_id`). The media has `live: true`, `title_id` and `asset_id` carry the TV channel id, there is no duration, nobody seeks or pauses, and every participant tunes the channel themselves (`GET /tv/channels/{id}/tune`, age check included). `PUT /watch-parties/{id}/media {tv_channel_id}` switches a party to a channel; a live channel cannot be queued. `sr-tv` answers the `PlayGate::resolve_live` check |

A system message in the channel ("@host started a watch party: *Akira* ▶ Join") updates live (viewers count, now playing).

## 3. Theatre UI
- Player (same as the Cinema player) + right panel with tabs **Chat** (the bound channel's chat, or the party's own chat for private parties) · **Queue** · **People**.
- Voice controls bar if bound to voice (mute/deafen/leave). Webcam bubbles over the stage while in a call, shown by default when a camera is live, hidden per account with Hide cameras (D-265).
- Reactions overlay (emoji float up on screen, rate-limited).
- "Ready check" before start (optional).
- Mini player when navigating elsewhere (Lounge, Library): playback continues in a floating PiP while the voice call continues (D-110).

## 4. Control model
- **Host** (creator) has full control. Host can transfer host, and can toggle **shared control**: everyone, or members with `CONTROL_WATCH_PARTY`, may play/pause/seek/skip.
- **Queue:** add titles/episodes (anyone, or host-only per setting), reorder (controllers), auto-advance, "play next episode" suggestion.
- **Sync protocol** (gateway op 100 `WATCH_PARTY_SYNC`, server-authoritative):
  - Server holds `{media_id, rendition_set, state: playing|paused|buffering, position_ms, at_server_ts, rate}`.
  - Clients estimate clock offset (NTP-style ping on join and every 30s) and compute the expected position = `position_ms + (now_server - at_server_ts) × rate`.
  - Drift handling: |drift| < 100 ms → ignore (the ignore band; a client already nudging keeps nudging until drift is under 40 ms, the hysteresis, so it does not flap at the edge). 100 ms–2 s → adjust playbackRate 0.95/1.05 until converged. > 2 s → hard seek. The band is 100 ms rather than 250 ms because the 10-client simulation needs viewers within 300 ms of each other (section 7.2, "Clock and drift").
  - **Buffering:** if any participant reports buffering > 1.5 s, the server may auto-pause everyone ("Waiting for @x…") (setting: on/off, max wait 10 s, then continue without them).
  - Late joiners seek to the expected position.
- Party ends when empty for 5 minutes (non-channel parties), or when the host ends it.

## 5. Limits
Concurrent parties per server by server level (doc 08). Party size ≤ the server's voice channel limit (if voice-bound), otherwise 50. Age-restricted titles require every participant to have confirmed 18+. Unconfirmed users see "This party is showing age-restricted content."

## 6. Acceptance criteria
- [ ] 10 viewers stay within ±300 ms of each other for a full film on mixed networks (sim test).
- [ ] Host pause/seek reflects on all clients within 500 ms.
- [ ] Navigating from the theatre to the Lounge keeps both the voice call and the mini player running.
- [ ] Shared-control off: non-hosts' play/pause buttons are disabled, and the server rejects their control ops.

## 7. HTTP and gateway API as built

Implemented by `crates/sr-watchparty` (rules, state machine, Postgres service) and the HTTP layer
`crates/sr-api/src/routes/watchparty`. Migration `0480_watch_parties.sql`. All routes are under `/api/v1`, need a
session, and use snowflakes as decimal strings. Times are unix milliseconds on the **server** clock.

### 7.1 Model
- **Party** `{id, host_id, binding, guild_id?, channel_id?, invite_code?, media, playback, settings, participants[], queue[], created_at_ms, ended, you?}`.
  `binding` is `channel` (a guild text or voice channel), `dm` (DM or group DM) or `private` (joined by link).
  `invite_code` is only returned to members of a private party; the share link is `/cinema/party/{id}?code={invite_code}`.
- **Media** `{title_id, episode_id ("0" = movie), variant_id?, asset_id, title, episode_label?, duration_ms?, age_restricted, live}`. The server resolves it with the catalog's play check (`CinemaService::resolve_play`): the title must be published and playable, a restricted one needs the caller's age confirmation (403, code 900011, D-184). **The party never hands out media URLs.** Every participant fetches HLS themselves through the normal `GET /cinema/assets/{asset_id}/playback` checks.
- **Playback** `{version, playing, position_ms, rate, at_ms, server_time_ms, expected_position_ms, auto_paused, duration_ms?, live}`. The media should be at `position_ms + (server_clock - at_ms) * rate` while `playing`. `version` increases by one with every change and orders the states; a client ignores anything not newer than what it applied. `server_time_ms` is stamped on every state.
- **Settings** `{shared_control, queue_policy, auto_pause_buffering, max_wait_ms, persistent, max_participants}`. `shared_control` and `queue_policy` are `host`, `controllers` (the host, participants the host named controllers, and members with `CONTROL_WATCH_PARTY` in the bound channel) or `everyone`. Defaults: shared control `host`, queue `controllers`, auto-pause on, max wait 10 s, 50 participants (at most the voice limit of a voice-bound channel).
- **You** `{member, host, can_control, can_queue, can_manage}` (present on REST reads, absent in gateway events).

### 7.2 Endpoints
| Method and path | Who | Notes |
|---|---|---|
| `GET /watch-parties/time` | any member | `{server_time_ms}`: clock sample for the offset estimate |
| `POST /watch-parties` | member | `{title_id, episode_id?, variant_id?, channel_id?, start_ms?, settings?}`. With `channel_id`: needs view and `START_WATCH_PARTY` (DM recipients always may). Without: private. 201. 409 if the channel already has a party or the server is at its limit (docs/08: 2/4/8/16 by level; level 0 until server levels exist). The caller leaves any other party |
| `GET /watch-parties/me` | member | the party the caller is in (404 if none) |
| `GET /watch-parties/{id}` | member or channel viewer | masked for non-members without age confirmation when restricted |
| `GET /channels/{channel_id}/watch-party` | channel viewer | the channel's live party |
| `PATCH /watch-parties/{id}` | host or channel manager | settings |
| `DELETE /watch-parties/{id}` | host or channel manager | end |
| `POST /watch-parties/{id}/join` | see below | `{invite_code?}` |
| `POST /watch-parties/{id}/leave` | participant | host role passes on |
| `POST /watch-parties/{id}/host` | host or manager | `{user_id}` handover |
| `PUT /watch-parties/{id}/controllers/{user_id}` | host or manager | `{controller}` |
| `DELETE /watch-parties/{id}/members/{user_id}` | host or manager | kick and bar |
| `POST /watch-parties/{id}/playback` | controllers (`ended`: any participant) | `{action: play, pause, seek, rate or ended; position_ms?, rate?, expected_version?}` returns the Playback. 403 without control, 409 `{..., playback}` on a stale `expected_version`, 429 over the rate limit |
| `PUT /watch-parties/{id}/media` | controllers | `{title_id, episode_id?, variant_id?, start_ms?}`; restricted media removes participants without age confirmation |
| `POST /watch-parties/{id}/queue` | per `queue_policy` | `{title_id, episode_id?, variant_id?}` (max 100) |
| `PATCH /watch-parties/{id}/queue/{item}` | controllers | `{position}` |
| `DELETE /watch-parties/{id}/queue/{item}` | per policy, or who added it | |
| `POST /watch-parties/{id}/next` | controllers | skip to the next queue entry (409 if empty) |
| `POST /watch-parties/{id}/heartbeat` | participant | `{rtt_ms?, buffering?, ready?}` returns `{server_time_ms, playback, member}`; send every 15 to 30 s |
| `POST /watch-parties/{id}/reactions` | participant | `{emoji}`, 8 per 5 s |
| `POST /watch-parties/{id}/messages` | participant | `{content}` ephemeral chat of private parties (not stored), 6 per 10 s |
| `GET /watch-parties/{id}/audit` | host or manager | party-level audit entries, newest first |

**Who may join.** Private: the invite code (404 otherwise, so parties cannot be probed). Bound: anyone who can see the channel. Always: not kicked, not full, and for a restricted title age-confirmed (else 403 900011; the client shows "This party is showing age-restricted content"). An account is in at most one party at a time: joining or creating leaves the previous party.

**Clock and drift.** Clients estimate `offset = server_time_ms - (client_now_at_response - rtt/2)` from `GET /watch-parties/time` and each heartbeat (median of recent samples) and compute the expected position with it. Drift thresholds as built in the web client: below 100 ms ignore, 100 ms to 2 s nudge the playback rate by 5 percent (0.95 or 1.05 times the party rate) until under 40 ms, above 2 s hard seek. The ignore band is 100 ms rather than the 250 ms the first draft of section 4 proposed because with 250 ms the 10-client simulation left two viewers up to 440 ms apart, and acceptance asks for 300 ms. Heartbeats carry `rtt_ms` so the participant list can show connection quality. The gateway op `100 WATCH_PARTY_SYNC` stays reserved: clock samples go over HTTP.

**Buffering.** A client that stalls more than about 1.5 s sends `heartbeat {buffering: true}` and `false` once it recovers. With `auto_pause_buffering` the server pauses everybody (`auto_paused: true`, "Waiting for ...") and resumes when nobody is stalled, or after `max_wait_ms` continues without the stalled participants. Reports are limited to 12 per minute per account.

**Lifecycle.** The host leaving hands the role to a controller, else the longest-standing participant. A party with nobody in it ends after 5 minutes unless `persistent` (server channel parties; the first person to join an empty persistent party becomes host). A participant silent for 90 s is dropped. The job `watchparty.sweep` (every 10 s) does the dropping, ending and wait settling; ended parties are deleted after 24 h. Audit entries are written for create, end, host changes, kicks, controller changes, settings and media changes (not for playback commands, which are rate limited instead: 12 per 5 s).

### 7.3 Gateway events (user-scoped dispatches to the participants)
- `WATCH_PARTY_UPDATE` `{type, party}` where `type` is `created`, `participant_joined`, `participant_left`, `participant_kicked`, `participant_updated` (ready, buffering, controller), `host_changed`, `settings`, `media`, `queue` or `ended`, and `party` is the full Party object (no `invite_code`, no `you`). `created`, `ended`, `media` and `host_changed` also go to everyone who can see the bound channel (restricted media masked) so a channel card updates live. `{type: "removed", party_id, reason}` (`kicked`, `age_restricted`) goes to a removed participant.
- `WATCH_PARTY_PLAYBACK` `{party_id, reason, by?, playback}` on every playback change (`play`, `pause`, `seek`, `rate`, `ended`, `media`, `buffering`, `resumed`, `continued`).
- `WATCH_PARTY_REACTION` `{party_id, user_id, emoji, at_ms}` and `WATCH_PARTY_MESSAGE` `{party_id, user_id, content, at_ms}`.

Events are hints for latency; the heartbeat answer and `GET /watch-parties/{id}` are the repair path.

### 7.4 Not in this build
A dedicated "Watch Party" channel *type* (D-208: the "Watch party room" preset covers it). Concurrent parties per server follow the server level (2, 4, 8, 16 for Level 0 to 3, `sr_server::watchparty::EntitlementLimits` over `Entitlements`, refreshed on `SR_ENTITLEMENT_UPDATE`).

### 7.4a The chat card (WP-BE-008, WP-FE-010)
A party bound to a channel has one **card**: a message of type **200** authored by the system user, with one embed (author "Watch party", the title, "episode · N watching", `url` `/cinema/party/<id>`; an age-restricted title is not named). The server posts it when the party starts and **rewrites it** (`MESSAGE_UPDATE`) when the media, the host or the headcount changes (at most one edit per 15 seconds per party, the newest change wins) and once more when the party ends (grey, "Ended after N viewers"). `watch_party_cards` (migration 1001) remembers the message of each party. Other clients show the embed. The web client draws the card from the live party state (the same gateway events the channel strip uses) while the party runs, with a Join button, and falls back to the embed once it ended. The opt-in announcer module (userfeat) still posts its own start and end text when enabled. A server can switch cards off with the guild module `community.watch_party_cards` (default on, Server Settings → Watch Parties & Library, D-304): a new party then posts nothing and records nothing, while a card that is already running is still rewritten and ended.

### 7.5 Phase 13 rules
- **Empty rooms (D-227).** A persistent party bound to a voice channel may have `media: null` and state `idle`. The room preset does not ask for a title; the first person to join an idle room hosts and picks the media, and the play gate, limits and age rule run at that moment. Ad-hoc parties still need media at creation.
- **Live party media (P12-TVW-3).** Live TV media is identified by `title_id` with `live: true` and is gated at tune time (the channel's own access rules), not at party creation.

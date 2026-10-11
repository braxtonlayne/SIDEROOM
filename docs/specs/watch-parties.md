# Spec — Watch Parties

Phase 5 · Decisions: D-110 · Permissions: `START_WATCH_PARTY`, `CONTROL_WATCH_PARTY` (doc 07).

## 1. What it is
Synchronized playback of Cinema content (VOD or a Live TV channel) for a group, with chat and voice. Each viewer streams the HLS media themselves from SIDE ROOM. Only **control state** is synchronized, so a party costs about the same as N people watching separately.

## 2. Where parties live
| Entry point | Result |
|---|---|
| Title page → **Watch together** | Choose: a Lounge text/voice channel you can post in, a DM/group DM, or "Private party" (shareable link to friends) |
| Lounge voice channel → **Start Watch Party** (call bar button) | Party bound to that voice channel. Voice continues as normal (E2EE call) |
| **Watch Party channel** type (server) | A voice channel variant whose main view is the theatre. The party persists while anyone is in it |
| Lounge message `<cinema:…>` card → **Watch together** | Starts a party in that channel |
| Live TV → **Watch together** | Party on a live channel (no seeking; everyone on the live edge) |

A system message in the channel ("@host started a watch party: *Akira* ▶ Join") updates live (viewers count, now playing).

## 3. Theatre UI
- Player (same as the Cinema player) + right panel with tabs **Chat** (the bound channel's chat, or the party's own chat for private parties) · **Queue** · **People**.
- Voice controls bar if bound to voice (mute/deafen/leave). Webcam bubbles as optional small overlays.
- Reactions overlay (emoji float up on screen, rate-limited).
- "Ready check" before start (optional).
- Mini player when navigating elsewhere (Lounge, Library): playback continues in a floating PiP while the voice call continues (D-110).

## 4. Control model
- **Host** (creator) has full control. Host can transfer host, and can toggle **shared control**: everyone, or members with `CONTROL_WATCH_PARTY`, may play/pause/seek/skip.
- **Queue:** add titles/episodes (anyone, or host-only per setting), reorder (controllers), auto-advance, "play next episode" suggestion.
- **Sync protocol** (gateway op 100 `WATCH_PARTY_SYNC`, server-authoritative):
  - Server holds `{media_id, rendition_set, state: playing|paused|buffering, position_ms, at_server_ts, rate}`.
  - Clients estimate clock offset (NTP-style ping on join and every 30s) and compute the expected position = `position_ms + (now_server - at_server_ts) × rate`.
  - Drift handling: |drift| < 250 ms → ignore. 250 ms–2 s → adjust playbackRate 0.95/1.05 until converged. > 2 s → hard seek.
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

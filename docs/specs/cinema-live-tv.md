# Spec — Cinema: Live TV ("A familiar frequency.")

Phase 5 · Theme `sideroom-broadcast` · Mockup `cinema-live-tv.jpg` · Decisions: D-100–D-103.

## 1. Concept
Nostalgic, always-on "channels" built entirely from the library. Each channel follows a **wall-clock schedule**: if the guide says *Cowboy Bebop* started at 8:00 PM, a viewer tuning in at 8:17 PM starts 17 minutes in (D-100). Nothing is delivered when no one is watching (D-101). There is **no runtime transcoding**: the live stream is a playlist stitched from already-transcoded segments (doc 02 §4.4).

Expected production lineup (the owner's words): several "Adult Swim" channels from different eras, a few anime channels, Cartoon Network/Nickelodeon-style era channels, a few genre movie channels, and filler channels.

## 2. Page layout (per mockup)
- Eyebrow "— THE TELEVISION ROOM," H1 "A familiar frequency.", `+ Create channel` (MANAGE_TV only). Remove the "SAMPLE LINEUP" tag.
- **Now-playing panel:** CRT-style preview frame (rounded bezel, scanlines, channel number top-left, channel logo/bug, "CHANNEL PREVIEW" label) playing the live stream muted on hover; eyebrow with channel name; program title; year · type; synopsis; `ⓘ Program details`; `▶ Tune in` (full-screen player); `Edit lineup` (MANAGE_TV); "● GOOD RECEPTION" status dot (green when the stream is healthy). Remove "Library preview · No live stream connected."
- **Filter chips:** All · Anime & animation · Movies · Series · Custom · Favorites.
- **Time navigation:** "← Evening lineup →" moves in 2-hour windows, plus a "Now" button.
- **Guide grid:** left column "YOUR CHANNELS" with local time "Now · 8:15 PM"; header row of half-hour slots in monospace (`8:00 PM`, `8:30 PM`…); rows per channel: ☆ favorite toggle, channel number (101, 102…), call sign/logo (`[adult swim]`, `CARTOON NETWORK`, …) and sub-label (network or "CUSTOM"); program cells sized by duration with "NOW · 8:00 PM" or the start time, title, and "Anime · 1998"; the current program is highlighted; a vertical **now-line** across the grid. Click a cell → program popover (synopsis, ▶ Tune in if now, "Remind me" if future, → title page).
- Footer notes: "Illustrative guide blocks · Channel tags do not indicate broadcast rights or current availability" → replace with operator-editable text (default: "Channels are assembled from the SIDE ROOM library.").
- Keyboard: ↑/↓ change channel, Enter tunes in, `G` toggles the guide overlay in the player.

## 3. Player (Live mode)
- Full-screen player with retro chrome: channel number OSD on tune (fades after 3s), channel up/down buttons, mini-guide overlay, "LIVE" dot, no seek bar (pause allowed. After resuming, a "Back to live" button jumps to the wall-clock position).
- Static/"tuning" transition (a short noise animation + sound, toggleable) between channels.
- Optional CRT filter (scanlines, slight curvature, vignette) toggle in player settings. Respects reduced motion.
- "Watch together" (watch party on a live channel = everyone is on the same live edge anyway; party adds voice/chat).

## 4. Channel types
| Kind | Definition |
|---|---|
| **imported_guide** | A historical lineup (e.g. "Adult Swim — Fall 2003") imported as a weekly schedule of slots referencing shows/episodes. Slots whose content isn't in the library are **skipped** (D-102): the previous program's end flows into the next available slot, and any gap is filled with filler per policy |
| **rule_based** | Rules generate the schedule: content pool (filters: genre, kind, decade, tags, collection), block length, ordering (sequential per series / shuffle / weighted), daypart rules (e.g. 22:00–02:00 only age-restricted allowed if the channel is restricted), reruns spacing |
| **manual** | Staff place programs on a calendar |

**Filler policy** for gaps: `bumpers` (short clips tagged `filler`), `next_program_early`, `slate` (a styled "We'll be right back" card with music), or `off_air` (color bars + tone, very retro).

## 5. Schedule engine (`sr-tv`)
- Channel timezone (D-103), default = operator timezone. The guide displays viewer-local times with a toggle to "channel time" (useful for era-authentic "8 PM block" channels).
- `resolve(channel, window) -> Vec<Block{start, end, asset_id, rendition_set, offset_ms}>` is deterministic: same inputs give the same schedule (seeded RNG per channel per week for shuffles). Results are cached per channel per day and invalidated on edits or library changes.
- Programs whose duration doesn't fit the slot: trim policy (`cut` at slot end / `overrun` and shift / `fit` with filler). Default `overrun` for series, `cut` never for movies.
- **Live playlist generation:** on `GET /tv/{channel}/live/{height}.m3u8`:
  1. now = server time. Find the current block and the segment index covering `now - block.start`.
  2. Emit a sliding-window media playlist (e.g. 6 segments) of signed URLs to existing VOD segments for that rendition, `#EXT-X-MEDIA-SEQUENCE` derived from wall-clock (so all viewers share sequence numbers), `#EXT-X-DISCONTINUITY` at program boundaries, `#EXT-X-PROGRAM-DATE-TIME` tags.
  3. A master playlist lists the 1080/720/480 variants (only those available for all upcoming blocks. If a program lacks a rung, the player drops to the common subset).
- No process runs per channel. Cost is one cheap computation per playlist request (a few per viewer per minute). "Dies once they leave" (D-101) is automatic.
- Guide API: `GET /sr/tv/guide?from&to&channels=` returns blocks for the grid. `GET /sr/tv/channels/{id}/now`.

## 6. Admin (MANAGE_TV)
Channel editor: number, call sign, name, era label, logo upload, kind, timezone, age-restricted flag, filler policy, rules editor (for rule_based), weekly grid editor (manual), **guide import** (CSV/JSON template with columns `weekday, start_time, duration, show, season?, episode?, note`; the importer fuzzy-matches shows to library titles and shows a mapping report: matched / ambiguous (pick) / missing (will be skipped)), schedule preview (next 7 days), "simulate now" time-travel for testing.

## 7. Acceptance criteria
- [ ] Two browsers tuning into the same channel at the same moment show the same frame within ±1 segment (6s).
- [ ] Joining 17 minutes into a program starts 17 minutes in.
- [ ] With zero viewers, no CPU or I/O is attributable to the channel (verified by metrics).
- [ ] An imported guide with 30% missing shows plays continuously with skips/filler and never errors.
- [ ] Safari (native HLS) handles discontinuities between programs with different audio layouts.

## 8. HTTP API as built (phase 8, crate `sr-tv`)

Owner: `p8-livetv-be` (crate `sr-tv`, routes in `sr-api::routes::tv`, tables `tv_channels` and `tv_blocks`, migrations 0460 and 0461). This section is the contract the web agent codes against; it is written before the code and kept in step with it (changes are listed in `docs/dev/handoff/feat-p8-livetv-be.md`). The OpenAPI document is the exact contract; this is the reading guide. **Nothing here has played a real stream**: playlists are verified against fixture assets and the in-repo playlist parser only.

Conventions (as Cinema): ids are Snowflakes and travel as decimal **strings**; timestamps are Unix **milliseconds** (`*_ms`); errors are the standard `{code, message}` body; every route needs a session (401 otherwise). Staff routes need a staff session holding `MANAGE_TV` (the Media Curator role has it); everything else is any signed-in member.

### 8.1 Concepts and scope

* A channel has a `kind`: `rule_based` (the schedule is generated from `rules`) or `manual` (staff place programmes). `imported_guide` (section 4), the guide import (TV-ADM-004), favorites, reminders and logo upload are **not built yet**; `logo_key` is stored and shown as a signed `/cdn/<key>` URL when set.
* The schedule is a list of non-overlapping **blocks** per channel. Rule-based channels are extended lazily and deterministically (same rules, same catalog, same seed give the same blocks): a read that needs blocks the channel does not have yet generates and stores them, up to 24 h ahead. Staff can also preview and publish a schedule explicitly (8.5).
* A block is `kind` `program` (a title or an episode from the Cinema catalog), `filler` (a short bumper), `slate` or `off_air` (no media; the client shows its own slate or colour bars). Gaps between programmes are filled by the channel's filler policy.
* Times of day in rules are in the channel `timezone` (any IANA zone name, resolved with `jiff` and its bundled tz database; `GET /tv/timezones` lists every zone). The guide itself is in absolute milliseconds; clients render viewer-local or channel time.
* Age restriction (D-184): restricted titles are scheduled on any channel whose rules include them and are always listed; **opening a stream** (`tune`, the live playlists) whose window contains a restricted programme answers **403 code 900011** until `POST /sr/age-gate {over_18:true}`.

### 8.2 Shapes

```
Block {
  id, channel_id, start_ms, end_ms,
  kind: "program"|"filler"|"slate"|"off_air",
  title,                       // series or film title ("Off air" / "We'll be right back" for slates)
  episode_title?, title_id?, episode_id?,
  media_kind?: "movie"|"series"|"anime",
  season_number?, episode_number?, year?,
  age_restricted, playable,    // playable: a stream exists for it (not a slate)
  source: "generated"|"manual"|"override"
}
Channel {
  id, number, name, call_sign?, era_label?, theme?, logo_url?, kind, timezone,
  age_restricted, enabled, filler_policy: "bumpers"|"next_program_early"|"slate"|"off_air",
  viewers,                     // distinct members who fetched a live playlist in the last 45 s
  now?: Block, next?: Block    // omitted by the admin list
}
```
`ChannelInput` (staff) is `{number (1..9999, unique), name, call_sign?, era_label?, theme?, logo_key? ("tv/..."), enabled, kind, timezone, age_restricted, filler_policy, rules}`; the staff `Channel` additionally carries `rules` and `seed`.

`rules`:
```
{ "grid_minutes": 30,             // programmes start on this grid in channel time; 0: back to back
  "no_repeat_window": 8,          // shuffle: the last N picks are not repeated (0..64)
  "dayparts": [ {
      "name": "Late night",       // unique, keys the sequential cursor
      "start": "22:00", "end": "02:00",   // channel time; end <= start wraps past midnight
      "days": [0,1,2,3,4,5,6],            // 0 = Monday; empty = every day. A wrapped daypart belongs to the day it starts on
      "mode": "shuffle"|"sequential",     // sequential keeps counting across days (series marathon: one title id)
      "fit": "overrun"|"fit",             // overrun: the last programme may run past `end`; fit: only items that fit
      "pool": { "kinds": ["movie","series","anime"], "genres": [], "tags": [], "title_ids": [],
                "year_from": null, "year_to": null, "collection_id": null,
                "min_runtime_min": null, "max_runtime_min": null,
                "restricted": "exclude"|"include"|"only"   // default exclude; include on an age_restricted channel
      } } ] }
```
Items without a runtime or an asset are never scheduled. Bumpers are published titles tagged `filler` with a runtime of at most 10 minutes.

### 8.3 Member routes

| Route | Result |
|---|---|
| `GET /tv/channels` | `{items: Channel[]}` enabled channels by number, each with `now` and `next` |
| `GET /tv/channels/{id}` | `Channel` |
| `GET /tv/channels/{id}/now` | `{channel_id, at_ms, offset_ms?, current?: Block, next?: Block, viewers}`; `offset_ms` is how far into the current block's programme the wall clock is (17 minutes in = 1020000) |
| `GET /tv/guide?from=&to=&channels=` | `{from_ms, to_ms, now_ms, channels: [{channel: Channel (no now/next), blocks: Block[]}]}`; `from` default now rounded down to the half hour, `to` default `from`+4 h, window at most 48 h (400 otherwise), `channels` is a comma list of ids (default all enabled). Blocks overlapping the window are returned whole |
| `GET /tv/timezones` | `{items: string[]}` |
| `GET /tv/channels/{id}/tune` | `{channel_id, state: "live"\|"off_air", master_url?, current?: Block, offset_ms?, next?: Block, viewers}`; 403 `900011` when the airing programme is restricted and the age is unconfirmed. `master_url` is the API path to use as the HLS source (see 8.4) |

### 8.4 Live HLS (`TV-HLS`)

Session-authenticated playlists (the player's HLS loader must send cookies, same origin). Each request computes the playlist from the wall clock; no process runs per channel.

| Route | Result |
|---|---|
| `GET /tv/channels/{id}/live/master.m3u8` | master playlist: one audio group (`aud`), variants for the heights **every programme of the next 3 hours has**, each `URI` `v/{height}.m3u8` (relative) |
| `GET /tv/channels/{id}/live/v/{height}.m3u8` | live video media playlist: sliding window of 6 fMP4 segments ending at the segment on air now, signed `/cdn/hls/<asset>/...` URLs (6 h), `#EXT-X-MEDIA-SEQUENCE` and `#EXT-X-DISCONTINUITY-SEQUENCE` derived from stored per-block bases (identical for every viewer), `#EXT-X-DISCONTINUITY` plus a new `#EXT-X-MAP` at every programme boundary, `#EXT-X-PROGRAM-DATE-TIME` on the first segment and after every discontinuity, no `ENDLIST`. A programme without that exact height uses the nearest rung |
| `GET /tv/channels/{id}/live/audio.m3u8` | the same window for each programme's default audio track |

Content type `application/vnd.apple.mpegurl`, `Cache-Control: no-store`. Off air (no playable block now): 404 with the message "Channel is off air". Disabled channel: 404. Fetching a media playlist counts the member as a viewer of the channel (45 s). Joining 17 minutes into a programme starts 17 minutes in because the window is cut at the segment containing the wall-clock position.

### 8.5 Staff routes (`/admin/tv/*`, `MANAGE_TV`)

Every successful change is written to the staff access log: `tv.channel.create|update|delete`, `tv.schedule.publish`, `tv.block.place|remove`, `tv.now.set`.

| Route | Body -> result |
|---|---|
| `GET /admin/tv/channels` | `{items: Channel[]}` all channels with `rules`, disabled too |
| `POST /admin/tv/channels` | `ChannelInput` -> 201 `Channel` |
| `GET\|PUT\|DELETE /admin/tv/channels/{id}` | `Channel`; `ChannelInput` -> `Channel`; 204 (deletes its schedule) |
| `GET /admin/tv/channels/{id}/schedule?from=&to=` | `{items: Block[]}` stored blocks overlapping the window (default now to +24 h, at most 7 days), no lazy extension |
| `POST /admin/tv/channels/{id}/preview` | `{from_ms?, to_ms?}` (default now to +24 h, at most 7 days) -> `{blocks: Block[], warnings: string[]}` generated from the *current* rules without storing anything, continuing the stored schedule's cursor (or starting at `from_ms`); `id` of preview blocks is `"0"` |
| `POST /admin/tv/channels/{id}/publish` | `{horizon_hours?: 1..336 default 168}` -> `{from_ms, to_ms, written, removed, warnings}`: removes the **generated** blocks that have not started, generates again from the programme on air now, keeps manual and override blocks (generated blocks are trimmed around them) |
| `POST /admin/tv/channels/{id}/blocks` | `{title_id, episode_id?, start_ms, end_ms?}` -> 201 `Block` places a programme (end defaults to start plus runtime); whatever it overlaps is trimmed, split or removed. `source` is `manual` on a manual channel, `override` otherwise |
| `DELETE /admin/tv/blocks/{id}` | 204 |
| `PUT /admin/tv/channels/{id}/now` | `{title_id, episode_id?, offset_ms?}` -> `Block`: starts that programme now (`offset_ms` into it), cutting the block on air; the schedule continues after it |
| `GET /admin/tv/channels/{id}/now?at_ms=` | as the member `now` but at any moment (time travel, "simulate now"), reads stored blocks only |
| `GET /admin/tv/report?from=&to=&channel=` | `{from_ms, to_ms, channels: [{channel_id, number, name, aired_ms, programmes, overrides}], items: [{block: Block, aired_ms}]}` what aired: stored blocks that started before now, clipped to the window (default the last 24 h, at most 31 days). Viewer figures are not persisted (D-101) |

Staff validation errors are 400 `{code: 50035}` with a message; a taken channel number is 409.

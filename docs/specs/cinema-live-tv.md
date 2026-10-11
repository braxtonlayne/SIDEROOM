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

# Spec — The Cinema (movies, series, anime)

Phase 5 · Theme `sideroom` · Mockups `cinema-discover.jpg`, `cinema-movies.jpg` · Decisions: D-075, D-080–D-086, D-089, D-120.

## 1. Navigation
Top bar: `SIDE ROOM / The lounge · **The cinema** · The library`. Cinema header: door mark + `SIDE ROOM` / `C I N E M A` · **Discover · Movies · Series · Anime ✦ · Live TV • · My list** · search · avatar (menu: Profile, My uploads, Upload, Requests, Settings, Back to Lounge). Footer: "Side Room *Cinema*" · "A little further from the ordinary." · About this library.

## 2. Pages

### 2.1 Discover (`/cinema`)
| Block | Behavior |
|---|---|
| **Hero ("TONIGHT'S FEATURE")** | Full-bleed backdrop, title in display serif (italic second line), year · runtime · rating badge · genre, synopsis, `▷ Play` (or `Resume 42:10`), `ⓘ More info`, `Watch trailer` if one exists, carousel index "01 / THE EVENING SELECTION" with auto-advance (pauses on hover). Staff-curated (Collections & Spotlight), falling back to trending |
| **Recently explored** | Your recently viewed or started titles |
| **A good place to start** | Curated row |
| **Animation. *Without limits.*** | Banner card with a fanned poster collage → Anime room |
| **The anime room** | Anime row with `ANIME` corner tag |
| **One more episode** | Series row (continue-watching first) |
| **For the love of cinema** | Films row |
| Generated rows | Continue watching · Trending (scope: Friends / People you know / Global) · Because you watched X · New this week · Friends are watching · Requests fulfilled |

Poster card: 2:3 art, hover lift + quick actions (▷, `+` My list, ✓ watched, rating), title, "2024 · Film" / "2022 · Series" / "1998 · Anime," and a progress bar if started.

### 2.2 Movies / Series / Anime (`/cinema/movies|series|anime`)
Per the Movies mockup: eyebrow "— THE SIDE ROOM LIBRARY," H1 "The cinema." (the Series and Anime pages use their own heading copy), subline "Films for a night well spent.", genre filter chips (All, Science fiction, Adventure, Fantasy, Drama, Thriller, Comedy, Horror…, single select + "More"), **"Room selection ▾"** dropdown = curated collections, title count ("10 titles"), grid of poster cards with `+`. Extra filters drawer: year range, rating, runtime, audio language, subtitles, age-restricted (if confirmed), sort.

### 2.3 Title detail (`/cinema/t/:id`)
Backdrop + logo or title, metadata row (year, runtime or seasons count, content rating, genres), synopsis, `▷ Play`/`Resume`, `+ My list`, rate, **Watch together** (start a watch party → choose a Lounge channel or create a private party link), share to Lounge, **Download** (Patrons only: rendition picker), trailer. Series: season selector, episode list (still, number, title, runtime, synopsis, progress, watched mark, "mark season watched"). Tabs: Episodes · Details (cast/crew text fields, studios, original title, audio/subtitle languages, variants such as "Director's Cut") · Reviews · Comments · More like this · Contributors.

### 2.4 My list (`/cinema/my-list`)
Saved titles + Continue watching + Watch history (pause/clear) + custom lists (shareable, visibility private/friends/everyone).

### 2.5 Requests, Upload, My uploads
See the submissions spec.

### 2.6 Live TV
See the cinema-live-tv spec.

## 3. Player
- hls.js (MSE) or native HLS (Safari/iOS). Adaptive 1080p/720p/480p ladder, manual quality override, data saver (cap 480p).
- Controls: play/pause, ±10s skip, scrubber with thumbnail sprites (generated during transcode: one frame every 10s, as a WebVTT sprite map), volume, playback speed 0.5–2×, audio track picker, subtitles (WebVTT + ASS via JASSUB, size/color/background styling), full screen, Picture-in-Picture, AirPlay/Remote Playback API, cast (Chromecast via the Web Sender SDK, optional), keyboard shortcuts (space, ←/→, ↑/↓, F, M, C, <, >).
- **Skip intro / skip credits** buttons from `chapters/markers` (set by reviewers or auto-detected later).
- **Next episode** card at credits with a 10s countdown. Autoplay toggle.
- Resume from `watch_progress` (saved every 10s and on pause/unload). Marked watched at ≥ 90%.
- "Are you still watching?" after 3 auto-played episodes.
- Watching presence → Lounge activity (D-120 privacy).
- Signed segment URLs expire after 6 hours and are refreshed transparently.

## 4. Ingest & transcoding (D-086)
1. Upload original (any size) → `video_assets.state = uploaded`.
2. Probe (`ffmpeg-next`): duration, streams, codecs, HDR flags, interlace detection, audio languages, embedded subtitles.
3. A low-res **review proxy** (480p, fast CPU encode or the first GPU job) lets reviewers watch before approval.
4. On approval: enqueue a **ladder job** → GPU worker produces:
   - H.264 High, CMAF/fMP4 HLS, 6s segments, 2s GOP, closed GOP aligned across rungs.
   - Renditions: 1080p ~6 Mbps, 720p ~3 Mbps, 480p ~1.5 Mbps (capped at the source resolution; **never above 1080p**). HDR → SDR tone-map (zscale/tonemap). Deinterlace when needed.
   - Audio: AAC-LC stereo 160 kbps per audio track (plus optional 5.1 AAC 384 kbps passthrough rung).
   - Subtitles → WebVTT. ASS also kept for styled rendering.
   - Thumbnail sprite sheet + VTT, poster frame candidates.
   - Segment index (durations) stored in `renditions.segment_index` for the Live TV stitcher.
5. Originals are kept (Storage Box or Garage cold bucket) for re-transcoding. The proxy is deleted after publishing.
6. Storage budget ≈ 4.7 GB per hour of content (doc 08).

## 5. Downloads (Patron, D-075)
Download = progressive MP4 remux of the chosen rendition (generated on demand by a job, cached for 7 days) or the original if staff enable "original downloads" per title. Links are signed and single-user, with a rate limit (e.g. 20 downloads/day).

## 6. Social
Ratings (1–10), reviews (spoiler toggle), episode comments, lists, trending scopes (Friends / Known / Global, same algorithm as the Library), "Friends are watching" row, contributor credits per movie and episode (D-074).

## 7. Acceptance criteria
- [ ] Discover and Movies match the mockups at desktop widths, with no "Board" link and no demo labels.
- [ ] A 4K HDR source becomes a 1080p SDR ladder. No rendition exceeds 1080p.
- [ ] Playback starts in < 2s on broadband, and switches quality without stalls on throttled networks (Playwright + network shaping).
- [ ] Resume position syncs across devices within 10s.
- [ ] Non-Patrons get 403 on download endpoints.
- [ ] Safari iOS (PWA) plays HLS with subtitles and resumes correctly.

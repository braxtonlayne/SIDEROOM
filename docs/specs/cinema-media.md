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
2. Probe (`ffprobe` JSON, see 4.1): duration, streams, codecs, HDR flags, interlace detection, audio languages, embedded subtitles.
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

### 4.1 Implementation map (CIN-ING-*)

The pipeline is pure logic behind ports, so it is unit-tested without FFmpeg. `ffmpeg-next` is **not linked** in the default build (ADR-0005 is honoured through the `native-ffmpeg` cargo feature, off by default and reserved for an in-process path); the default path spawns the `ffprobe`/`ffmpeg` command-line tools with argument vectors (never a shell) through `sr_media::runner::CommandRunner`.

| Item | Where |
|---|---|
| Probe (001) | `sr-media::probe`: `ffprobe -print_format json` parsed into `ProbeResult` (4 MiB JSON cap, 256-stream cap, sanitised strings, lenient numbers) |
| Review proxy plan (002), ladder planner (003) | `sr-media::plan`: `plan_proxy_job`, `plan_ladder_job` |
| Worker API (004) | `sr-media::worker_api` (types, `JobStore`/`OutputStorage` ports, `WorkerService`, in-memory stores) and `sr-media::http` (axum router) |
| Transcoder (005) | `sr-transcoder`: `encoders` (NVENC detection), `args` (ffmpeg argument vectors), `progress`, `worker` (state machine), `http_client` |
| Playlists (007) | `sr-media::playlist` |
| ASS/SRT to WebVTT, sprite map (008) | `sr-media::subtitles` |
| Retention (009), re-transcode (011) | `sr-media::retention`, `sr-media::retranscode` |
| Images and guide (006) | `deploy/docker/transcoder*.Dockerfile`, `deploy/TRANSCODER.md` |

### 4.2 Planning rules

- **Ladder**: the templates are 1080p (6000 kbps), 720p (3000), 480p (1500), each a bounding box (1920x1080, 1280x720, 854x480). A rung exists when its box is not larger than the source in both dimensions (so a 1920x800 scope film keeps a full-width 1080p-class rung of 1920x800), never above `max_height` (1080), never upscaled; sample aspect ratio is applied first (anamorphic DVD). A source smaller than every rung gets one native-size rung. Bitrate scales with the pixel area actually used (floor 20%).
- **Frame rate and GOP**: one constant output rate for all rungs (23.976/29.97/59.94 snap to their fractions, above 60 halves); GOP = 2 s of frames (`48` at 23.976), `scenecut` off, closed GOP, so keyframes land on identical frames in every rung and 6 s segments are 3 GOPs. The profile version (`PROFILE_VERSION`) is stored per rendition for "re-transcode everything older than profile N".
- **HDR to SDR**: HDR10, HDR10+, HLG and Dolby Vision profiles with an HDR10/HLG base layer go through `zscale` + `tonemap=hable` per rung (after scaling, to keep the float RGB work small). Dolby Vision profile 5 has no base layer and needs a `libplacebo` build; the planner warns and the worker refuses the job (non-retryable) when the filter is missing.
- **Audio**: one stereo AAC (160 kbps, 48 kHz) per language (the track with the most channels; commentary dropped unless `keep_commentary`), preferred languages first, at most 8; optional 5.1 AAC (384 kbps) when `keep_surround` and the source has 6+ channels. Each is its own audio-only fMP4 HLS rendition (EXT-X-MEDIA group `aud`).
- **Subtitles**: text tracks (SRT, ASS/SSA, WebVTT, mov_text) are extracted by ffmpeg (ASS to `.ass`, others to `.srt`) and converted to WebVTT by the Rust converter; ASS files are retained and embedded fonts are dumped to `fonts/`. Bitmap tracks (PGS, VobSub, DVB) are recorded in `skipped_subtitles` as `image_based_ocr_later`.
- **Sprites and posters**: one thumbnail every 10 s, 160 px wide, 10x10 tiles per JPEG sheet, plus a WebVTT map with `#xywh=` fragments; 5 poster candidates between 10% and 70% of the runtime.
- **Proxy**: one MP4, at most 854x480 and 30 fps, `libx264 -preset veryfast -crf 30` (or `h264_nvenc -preset p1`), AAC 96 kbps, `+faststart`.

### 4.3 Output layout (relative to the asset root, forward slashes)

```
v/<rung>/index.m3u8 init.mp4 seg_00000.m4s ...     video renditions (1080p, 720p, 480p)
a/<nn>-<lang>-<stereo|51>/index.m3u8 init.mp4 seg_*.m4s   audio renditions
subs/<nn>-<lang>.vtt (+ .ass when the source was ASS)
fonts/<name>.ttf|otf|...                             embedded fonts (ASS only)
sprites/sheet-001.jpg ... and sprites/sprites.vtt
posters/candidate-1.jpg ...
proxy/proxy.mp4                                      review proxy job only
```

### 4.4 Worker API

All routes are under `/api/v1/transcode/worker` and need `Authorization: Bearer <worker token>`. The token is created in Admin → Transcoding; only its SHA-256 is stored. Errors are `{ "code", "message" }` with the statuses below. The routes are built by `sr_media::http::router(Arc<WorkerService>)` and merged into sr-server by the catalog work once a Postgres `JobStore` exists (they are deliberately **not** wired into sr-server in this change, so `openapi.json` is unchanged).

| Method and path | Purpose | Body → response |
|---|---|---|
| `POST /register` | Announce name, version, capabilities (NVENC, encoders, concurrency, libplacebo). Same token + name keeps the worker id | `RegisterRequest` → `{ worker_id, heartbeat_secs, lease_secs, poll_secs }` |
| `POST /workers/{worker_id}/heartbeat` | Liveness; also recycles jobs whose lease expired | `{ active_jobs }` → `{ cancel_jobs[], drain }` |
| `POST /workers/{worker_id}/claim` | Atomically claim the best queued job (priority desc, then age; proxies are queued with high priority) | `{ kinds[] }` → `{ job: null \| ClaimedJob }` |
| `POST /jobs/{job_id}/progress` | Stage, percent, fps, speed; extends the lease | `ProgressRequest` → `{ cancel, lease_expires_at }` |
| `POST /jobs/{job_id}/uploads` | Start a presigned multipart upload of one output | `{ path, size }` → `{ upload_id, part_size, parts[{part_number,url}], content_type, expires_at }` |
| `POST /jobs/{job_id}/uploads/{upload_id}/complete` | Finish it with the part ETags | `{ parts[{part_number,etag}] }` → `{ ok }` |
| `POST /jobs/{job_id}/complete` | Finish the job: output list, segment index per rendition, encoder, wall time, log tail | `CompleteRequest` → `{ ok }` |
| `POST /jobs/{job_id}/fail` | Fail (retryable or not) or acknowledge a cancel, with stage, message and log tail | `FailRequest` → `{ ok }` |

`ClaimedJob` carries the job id, attempt, `spec` (`proxy` plan or `ladder` plan, so the worker needs no server-side logic), a **signed GET URL of the original** (8 h, `/cdn/originals/<key>?ex=&sig=`), its size and extension, the lease expiry and a maximum output budget (max of 20 GiB and 2x the original).

Rules the service enforces:
- **Lease**: 120 s, extended by every progress call. After expiry the job is requeued (attempt counter kept; 3 attempts) and the old worker gets `410 lease_expired` and must stop silently. `403`/`404`/`409`/`410` all mean "this job is no longer yours".
- **Output sandbox**: relative paths of letters, digits and `._- `, at most five components, no dot-files, a known extension (`m3u8 mp4 m4s vtt ass jpg ttf otf ttc woff woff2`), and under a directory the job's plan allows. The content type comes from the extension, never from the client. Single files up to 4 GiB, parts 16 MiB (max 10,000), at most 200,000 files, total within the job budget.
- **Staging keys**: outputs land at `transcode/<job_id>/<attempt>/<path>`; the catalog promotes them to the asset's final prefix when it accepts `complete`.
- **Completion check**: every path in the plan's required outputs was uploaded and finished, declared sizes match, each rung has a non-empty segment index whose segments exist. Logs are clipped to 32 KiB with control characters removed.
- **Cancel**: an admin cancel of a queued job is immediate; for a running job the flag is returned by `progress` and `heartbeat`, and the worker answers `fail` with `cancelled: true`.
- Presigned multipart needs part-level presigning (S3 SigV4 `UploadPart`). `ObjectStorage::presign` today only signs whole-object PUTs; until a part presigner exists the `OutputStorage` implementation can set `part_size` above the largest output (every output then has exactly one part, i.e. one presigned PUT).

### 4.5 Proposed tables (not created here; migrations 0420 onward belong to the catalog work)

```sql
CREATE TABLE worker_tokens   (id bigint PRIMARY KEY, name text NOT NULL, token_sha256 text NOT NULL UNIQUE,
                              created_by bigint NOT NULL, created_at timestamptz NOT NULL DEFAULT now(), revoked_at timestamptz);
CREATE TABLE transcode_workers(id text PRIMARY KEY, token_id bigint NOT NULL REFERENCES worker_tokens(id),
                              name text NOT NULL, version text NOT NULL, capabilities jsonb NOT NULL,
                              last_seen_at timestamptz NOT NULL, drain boolean NOT NULL DEFAULT false,
                              UNIQUE (token_id, name));
CREATE TABLE video_assets    (id bigint PRIMARY KEY, title_id bigint, episode_id bigint,
                              state text NOT NULL,            -- uploaded|probing|proxy_ready|in_review|approved|transcoding|published|rejected|removed|failed
                              original_key text, original_size bigint, original_tier text NOT NULL DEFAULT 'hot',
                              probe jsonb, duration_ms bigint, profile_version int,
                              published_at timestamptz, ended_at timestamptz, original_downloads boolean NOT NULL DEFAULT false);
CREATE TABLE renditions      (id bigint PRIMARY KEY, asset_id bigint NOT NULL REFERENCES video_assets(id) ON DELETE CASCADE,
                              kind text NOT NULL,             -- video|audio|subtitle|sprite|poster|proxy
                              name text NOT NULL, playlist_key text, width int, height int, bandwidth_bps bigint,
                              codecs text, language text, label text, is_default boolean, forced boolean,
                              segment_index jsonb, profile_version int NOT NULL);
CREATE TABLE transcode_jobs  (id bigint PRIMARY KEY, asset_id bigint NOT NULL REFERENCES video_assets(id) ON DELETE CASCADE,
                              kind text NOT NULL, spec jsonb NOT NULL, state text NOT NULL,   -- queued|running|succeeded|failed|cancelled
                              priority smallint NOT NULL, attempt int NOT NULL DEFAULT 0, worker_id text REFERENCES transcode_workers(id),
                              lease_expires_at timestamptz, cancel_requested boolean NOT NULL DEFAULT false,
                              progress jsonb, error text, log_tail text, encoder text,
                              created_at timestamptz NOT NULL DEFAULT now(), finished_at timestamptz);
CREATE INDEX transcode_jobs_claim ON transcode_jobs (priority DESC, id) WHERE state = 'queued';
CREATE TABLE transcode_uploads(job_id bigint NOT NULL REFERENCES transcode_jobs(id) ON DELETE CASCADE,
                              upload_id text NOT NULL, path text NOT NULL, key text NOT NULL, size bigint NOT NULL,
                              finished boolean NOT NULL DEFAULT false, PRIMARY KEY (job_id, path));
```

`claim_next` is one statement: `UPDATE transcode_jobs SET state='running', attempt=attempt+1, worker_id=$1, lease_expires_at=now()+$2 WHERE id = (SELECT id FROM transcode_jobs WHERE state='queued' AND kind = ANY($3) ORDER BY priority DESC, id FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING *`.

### 4.6 Playlists and signed URLs

`sr-media::playlist` renders the master playlist (audio group `aud`, subtitle group `subs`, variants ascending by bandwidth so playback starts on the smallest rung), the fMP4 media playlists, the one-segment subtitle playlists and the sprite VTT. Every URL in a rendering is signed with the same expiry, `now + 6 h` (`UrlSigner`, implemented by `CdnSigner`). `grant()` returns `{ master_url, expires_at, refresh_at }` (refresh 10 minutes early); the playback endpoint calls it after the permission check and the refresh endpoint calls it again with the same check. Nothing is stored per viewer.

### 4.7 Not exercised with real ffmpeg

No ffmpeg or ffprobe was available when this was written. The argument vectors, the filter graphs (including the zscale/tonemap and libplacebo chains, the `-dump_attachment` font extraction and the HLS muxer's init-segment placement), the NVENC options and the progress parser are checked against **canned output and fake runners only**. First run on a machine with FFmpeg 6 or newer should be a `sr-transcoder --check` and one proxy plus one ladder job on a short sample, comparing the produced tree with section 4.3.

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

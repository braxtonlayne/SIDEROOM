# 14 — Cinema (`CIN`)

Phase 5. Spec: [cinema-media](../specs/cinema-media.md). Mockups: `cinema-discover.jpg`, `cinema-movies.jpg`.

## CIN-DATA — Catalog backend
- [x] **CIN-DATA-001** [DB] video_titles, seasons, episodes, video_assets, renditions, audio_tracks, subtitle_tracks, trailers, markers, watch_progress, transcode_jobs, collections (catalog side done in migration 0370: titles, seasons, episodes, variants, trailers, markers, collections, collection items; the asset/rendition/track/job tables belong to CIN-ING and `watch_progress` to CIN-PLY-007, they reference `asset_id` / `title_id` / `episode_id` without foreign keys)
- [x] **CIN-DATA-002** [BE] Title/season/episode CRUD (publishers), publish/unpublish, merge duplicates, variants (`/admin/cinema/*`, merge duplicates, variants, markers, trailers, artwork upload reservation, access log)
- [x] **CIN-DATA-003** [BE] Catalog API (discover rows, browse by kind with filters/sort, title detail, episodes, search) (`/cinema/*`: discover, rows, browse with keyset paging, facets, title detail, episodes, play target, search; spec section 8)
- [x] **CIN-DATA-004** [BE] Tantivy index for titles (title, original title, cast/crew text, tags) (tantivy `TitleIndex`, rebuilt from Postgres at boot, `/cinema/search`)
- [x] **CIN-DATA-005** [BE] Collections/rows API (curated + generated: continue watching, trending scoped, because you watched, new this week, friends are watching) (curated collections + generated rows done; continue watching, trending, because you watched and friends are watching plug in through `RowProvider`, to be implemented by CIN-SOC-002)
- [x] **CIN-DATA-006** [BE] Age-restricted filtering (`age=hide|include|only`, `cinema.hide_age_restricted`, 403 900011 on `/play`)

## CIN-ING — Ingest & transcoding
- [x] **CIN-ING-001** [BE] Probe service (`ffmpeg-next`): streams, codecs, duration, HDR, interlace, languages, embedded subs · *parser tested on canned ffprobe JSON; ffprobe never run*
- [x] **CIN-ING-002** [BE] Review proxy job (480p fast encode) for reviewers · *logic and tests only, not exercised with real ffmpeg*
- [x] **CIN-ING-003** [BE] Transcode job planner (ladder ≤ source ≤ 1080p, audio tracks, subtitle extraction, sprites) · *logic and tests only, not exercised with real ffmpeg*
- [x] **CIN-ING-004** [BE] Worker API: register/heartbeat, claim job, signed download of original, presigned multipart uploads for outputs, progress, complete/fail with logs · *Postgres `JobStore` (SKIP LOCKED claim, leases, 3 attempts, reaper job), S3 multipart `OutputStorage` with part-level SigV4 presigning, tokens, mounted in sr-server; Postgres-tested with fakes, never against a real S3 or worker*
- [x] **CIN-ING-005** [BE] `sr-transcoder` binary: NVENC detection, h264_nvenc / libx264 fallback, scale, HDR→SDR tonemap, deinterlace, closed aligned GOPs, fMP4 HLS 6s segments, AAC tracks, WebVTT subs, thumbnail sprite + VTT, segment index output · *logic and tests only, not exercised with real ffmpeg*
- [x] **CIN-ING-006** [OPS] Transcoder CUDA container image + CPU image + "rent a GPU" guide · *Dockerfiles and guide written (`deploy/TRANSCODER.md`); images not built here*
- [x] **CIN-ING-007** [BE] Master/media playlist generation with signed segment URLs (6h), refresh endpoint · *pure functions and tests; phase 7 added `GET /cinema/assets/{id}/playback` and `POST .../refresh` (catalog play check and age gate first) and the `/cdn/hls` delivery of rendered playlists; the Postgres `PlaybackSource` over the ingest tables is hooked up at merge time (spec section 10)*
- [x] **CIN-ING-008** [BE] ASS subtitle retention + WebVTT conversion · *converter tested on hand-written ASS/SRT, not on real ffmpeg-extracted files*
- [x] **CIN-ING-009** [BE] Originals retention (cold bucket / Storage Box), proxy cleanup after publish · *decisions plus the hourly `cinema.media.sweep` job (originals, proxies, slots, prefixes); cold-tier moves are only counted (no second bucket)*
- [x] **CIN-ING-010** [BE] Cinema `SubmissionKind` impls (title, episodes batch) with publish actions · *`cinema.title` and `cinema.episodes` registered with sr-review and Postgres-tested; the member draft-edit and upload routes are not built, so only staff-uploaded assets can be referenced*
- [x] **CIN-ING-011** [BE] Re-transcode action (single asset / whole title / all — admin) · *planner plus `POST /admin/transcode/retranscode` that enqueues; not run against real ffmpeg*

## CIN-UI — Section chrome
- [x] **CIN-UI-001** [FE] Section top bar with "The cinema" active
- [x] **CIN-UI-002** [FE] Cinema header: logo `SIDE ROOM / C I N E M A`, nav Discover · Movies · Series · Anime ✦ · Live TV • · My list (active underline), search icon → overlay, avatar menu (Profile, My uploads, Upload, Requests, Settings, Back to Lounge)
- [x] **CIN-UI-003** [FE] Search overlay (instant results with posters, filters shortcut) (the Filters shortcut opens the Movies filters drawer)
- [x] **CIN-UI-004** [FE] Footer ("Side Room *Cinema*", "A little further from the ordinary.", About this library)
- [x] **CIN-UI-005** [FE] Mobile layout (hamburger nav, stacked rows)

## CIN-DISC — Discover
- [x] **CIN-DISC-001** [FE] Hero "TONIGHT'S FEATURE" (backdrop, display title w/ italic second line, meta row with rating badge, synopsis, Play/Resume, More info, Watch trailer, "01 / THE EVENING SELECTION" index, auto-advance with pause on hover)
- [x] **CIN-DISC-002** [FE] Row: Recently explored
- [x] **CIN-DISC-003** [FE] Row: A good place to start (+ "SELECTED FOR THE ROOM" label, Explore all ↗)
- [x] **CIN-DISC-004** [FE] Animation banner card ("A ROOM OF ITS OWN", "Animation. *Without limits.*", copy, "Step into the anime room ↗", fanned poster collage)
- [x] **CIN-DISC-005** [FE] Row: The anime room (ANIME corner tags)
- [x] **CIN-DISC-006** [FE] Row: One more episode
- [x] **CIN-DISC-007** [FE] Row: For the love of cinema
- [x] **CIN-DISC-008** [FE] Generated rows: Continue watching, Trending (scope switch), Because you watched, New this week, Friends are watching
- [x] **CIN-DISC-009** [FE] Poster card hover actions (▷ play, + My list, ✓ watched, rating) + progress bar

## CIN-BRW — Movies / Series / Anime
- [x] **CIN-BRW-001** [FE] Page header (eyebrow "— THE SIDE ROOM LIBRARY", H1, subline) per kind
- [x] **CIN-BRW-002** [FE] Genre chips (single select, All) + More
- [x] **CIN-BRW-003** [FE] "Room selection ▾" collections dropdown
- [x] **CIN-BRW-004** [FE] Filters drawer (year range, rating, runtime, audio language, subtitles, age-restricted, sort)
- [x] **CIN-BRW-005** [FE] Title count + poster grid (6-col desktop, responsive), "+" add to list
- [x] **CIN-BRW-006** [FE] Anime page ✦ accent styling

## CIN-TTL — Title detail
- [x] **CIN-TTL-001** [FE] Header: backdrop, logo/title, meta row, synopsis, Play/Resume, + My list, Rate, Watch together, Share to Lounge, Download (Patron), Trailer (Watch together, Share to Lounge copy-link and Download are stubs: watch parties, Lounge share card and patron downloads are not built)
- [x] **CIN-TTL-002** [FE] Season selector + episode list (still, number, title, runtime, synopsis, progress, watched toggle, mark season watched)
- [x] **CIN-TTL-003** [FE] Tabs: Episodes, Details (cast/crew text, studios, languages, variants), Reviews, Comments, More like this, Contributors (Contributors tab stays disabled until contribution credits exist, CIN-SOC-003)
- [x] **CIN-TTL-004** [FE] Variant picker (e.g. Director's Cut)

## CIN-PLY — Player
- [x] **CIN-PLY-001** [FE] hls.js / native HLS engine wrapper, ABR with manual override, data saver
- [x] **CIN-PLY-002** [FE] Controls: play/pause, ±10s, scrubber with sprite thumbnails, volume, speed, audio track menu, subtitle menu + styling, full-screen, PiP, Remote Playback/AirPlay, optional Chromecast (Chromecast Web Sender SDK not included; Remote Playback and AirPlay are)
- [x] **CIN-PLY-003** [FE] Keyboard shortcuts + on-screen feedback
- [ ] **CIN-PLY-004** [FE] ASS subtitle renderer (JASSUB, worker) + WebVTT
- [x] **CIN-PLY-005** [FE] Skip intro / skip credits buttons (markers)
- [x] **CIN-PLY-006** [FE] Next episode card with countdown, autoplay toggle, "Are you still watching?"
- [x] **CIN-PLY-007** [BE] Watch progress API (10s heartbeat, ≥ 90% watched) + cross-device sync *(phase 6: `/cinema/titles/{id}[/episodes/{eid}]/progress`, history, continue watching, `SR_CINEMA_UPDATE`; spec section 8)*
- [x] **CIN-PLY-008** [FE] Resume prompt ("Resume from 42:10 / Start over")
- [x] **CIN-PLY-009** [BE] "Watching" presence emission (privacy-aware) *(phase 6: `SR_ACTIVITY_UPDATE` and `GET /cinema/me/friends-watching`, setting `cinema.share_watching`; folding it into `PRESENCE_UPDATE.activities` is a gateway follow-up)*
- [x] **CIN-PLY-010** [FE] Signed URL refresh without playback interruption (re-signs hls.js requests from the refreshed playlists; native HLS swaps the source)
- [x] **CIN-PLY-011** [FE] Mini player (continue while navigating, also used by watch parties) (mounted at the app root; watch parties reuse `playerSession()` and `MiniPlayerView`)
## CIN-MY — My list & history
- [x] **CIN-MY-001** [FE] My list page (saved, continue watching, custom lists, history with pause/clear)
- [x] **CIN-MY-002** [BE] Lists API (shared with Library lists implementation where possible) *(phase 6: My list and custom lists in `sr-cinema-social`; the Library has no lists yet, so nothing was shared)*

## CIN-SOC — Social
- [x] **CIN-SOC-001** [BE] Ratings, reviews, episode comments (reuse LIB-SOC services generically) *(phase 6: ratings 1-10, reviews with spoiler flag and helpful votes, threaded comments with likes, reports and a moderation hook; phase 7: staff report queue, hide/restore and resolve under `/admin/cinema/*` with access-log entries, `RepoCatalog` wiring, merge and delete cascade)*
- [x] **CIN-SOC-002** [BE] Trending (scoped) + recommendations *(phase 6: scoped trending and "because you watched" recommendations)*
- [ ] **CIN-SOC-003** [BE] Contribution credits for movies/episodes *(not built in phase 6: needs the cinema submissions pipeline, see cinema-media section 8.8)*

## CIN-DL — Patron downloads
- [x] **CIN-DL-001** [BE] On-demand MP4 remux job of chosen rendition (cached 7 days), original download when allowed, signed single-user link, daily limit · *remux job kind (worker side faked, not run with real ffmpeg), cache, owner-only link redirecting to a short signed URL, daily limit from `cinema_download_grants`; the perk source (donations) does not exist yet*
- [x] **CIN-DL-002** [FE] Download modal (rendition picker with sizes, progress of prep job) (the modal is `cinema/player/DownloadModal.tsx`, now mounted by the title page header for films and per episode; the download routes it calls are not in the backend yet, so it shows the error copy)

## CIN-ADM — Admin
- [x] **CIN-ADM-001** [FE] Catalog editor (title form, seasons/episodes tables, assets & renditions view, subtitles manager, artwork, markers editor with video scrubber, publish toggle, merge) (`admin/cinema/`; the assets & renditions view is the asset link plus transcode status per asset; there is no subtitles manager because no subtitle catalog route exists; the marker preview needs the ingest playback route)
- [x] **CIN-ADM-002** [FE] Transcode status per asset + re-run (screens and data layer done against an assumed `/admin/transcode/*` contract; they show "not available yet" until the ingest-db backend ships, see `transcodeApi.ts`)

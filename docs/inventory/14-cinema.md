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
- [x] **CIN-ING-004** [BE] Worker API: register/heartbeat, claim job, signed download of original, presigned multipart uploads for outputs, progress, complete/fail with logs · *contract, service, routes and in-memory store done and tested; the Postgres `JobStore` and sr-server wiring are the catalog work*
- [x] **CIN-ING-005** [BE] `sr-transcoder` binary: NVENC detection, h264_nvenc / libx264 fallback, scale, HDR→SDR tonemap, deinterlace, closed aligned GOPs, fMP4 HLS 6s segments, AAC tracks, WebVTT subs, thumbnail sprite + VTT, segment index output · *logic and tests only, not exercised with real ffmpeg*
- [x] **CIN-ING-006** [OPS] Transcoder CUDA container image + CPU image + "rent a GPU" guide · *Dockerfiles and guide written (`deploy/TRANSCODER.md`); images not built here*
- [x] **CIN-ING-007** [BE] Master/media playlist generation with signed segment URLs (6h), refresh endpoint · *pure functions and tests; the playback and refresh endpoints call them once the catalog exists*
- [x] **CIN-ING-008** [BE] ASS subtitle retention + WebVTT conversion · *converter tested on hand-written ASS/SRT, not on real ffmpeg-extracted files*
- [x] **CIN-ING-009** [BE] Originals retention (cold bucket / Storage Box), proxy cleanup after publish · *pure decision logic and tests; the sweep job is catalog work*
- [ ] **CIN-ING-010** [BE] Cinema `SubmissionKind` impls (title, episodes batch) with publish actions
- [x] **CIN-ING-011** [BE] Re-transcode action (single asset / whole title / all — admin) · *pure planner and tests; the admin action and job enqueue are catalog work*

## CIN-UI — Section chrome
- [ ] **CIN-UI-001** [FE] Section top bar with "The cinema" active
- [ ] **CIN-UI-002** [FE] Cinema header: logo `SIDE ROOM / C I N E M A`, nav Discover · Movies · Series · Anime ✦ · Live TV • · My list (active underline), search icon → overlay, avatar menu (Profile, My uploads, Upload, Requests, Settings, Back to Lounge)
- [ ] **CIN-UI-003** [FE] Search overlay (instant results with posters, filters shortcut)
- [ ] **CIN-UI-004** [FE] Footer ("Side Room *Cinema*", "A little further from the ordinary.", About this library)
- [ ] **CIN-UI-005** [FE] Mobile layout (hamburger nav, stacked rows)

## CIN-DISC — Discover
- [ ] **CIN-DISC-001** [FE] Hero "TONIGHT'S FEATURE" (backdrop, display title w/ italic second line, meta row with rating badge, synopsis, Play/Resume, More info, Watch trailer, "01 / THE EVENING SELECTION" index, auto-advance with pause on hover)
- [ ] **CIN-DISC-002** [FE] Row: Recently explored
- [ ] **CIN-DISC-003** [FE] Row: A good place to start (+ "SELECTED FOR THE ROOM" label, Explore all ↗)
- [ ] **CIN-DISC-004** [FE] Animation banner card ("A ROOM OF ITS OWN", "Animation. *Without limits.*", copy, "Step into the anime room ↗", fanned poster collage)
- [ ] **CIN-DISC-005** [FE] Row: The anime room (ANIME corner tags)
- [ ] **CIN-DISC-006** [FE] Row: One more episode
- [ ] **CIN-DISC-007** [FE] Row: For the love of cinema
- [ ] **CIN-DISC-008** [FE] Generated rows: Continue watching, Trending (scope switch), Because you watched, New this week, Friends are watching
- [ ] **CIN-DISC-009** [FE] Poster card hover actions (▷ play, + My list, ✓ watched, rating) + progress bar

## CIN-BRW — Movies / Series / Anime
- [ ] **CIN-BRW-001** [FE] Page header (eyebrow "— THE SIDE ROOM LIBRARY", H1, subline) per kind
- [ ] **CIN-BRW-002** [FE] Genre chips (single select, All) + More
- [ ] **CIN-BRW-003** [FE] "Room selection ▾" collections dropdown
- [ ] **CIN-BRW-004** [FE] Filters drawer (year range, rating, runtime, audio language, subtitles, age-restricted, sort)
- [ ] **CIN-BRW-005** [FE] Title count + poster grid (6-col desktop, responsive), "+" add to list
- [ ] **CIN-BRW-006** [FE] Anime page ✦ accent styling

## CIN-TTL — Title detail
- [ ] **CIN-TTL-001** [FE] Header: backdrop, logo/title, meta row, synopsis, Play/Resume, + My list, Rate, Watch together, Share to Lounge, Download (Patron), Trailer
- [ ] **CIN-TTL-002** [FE] Season selector + episode list (still, number, title, runtime, synopsis, progress, watched toggle, mark season watched)
- [ ] **CIN-TTL-003** [FE] Tabs: Episodes, Details (cast/crew text, studios, languages, variants), Reviews, Comments, More like this, Contributors
- [ ] **CIN-TTL-004** [FE] Variant picker (e.g. Director's Cut)

## CIN-PLY — Player
- [ ] **CIN-PLY-001** [FE] hls.js / native HLS engine wrapper, ABR with manual override, data saver
- [ ] **CIN-PLY-002** [FE] Controls: play/pause, ±10s, scrubber with sprite thumbnails, volume, speed, audio track menu, subtitle menu + styling, full-screen, PiP, Remote Playback/AirPlay, optional Chromecast
- [ ] **CIN-PLY-003** [FE] Keyboard shortcuts + on-screen feedback
- [ ] **CIN-PLY-004** [FE] ASS subtitle renderer (JASSUB, worker) + WebVTT
- [ ] **CIN-PLY-005** [FE] Skip intro / skip credits buttons (markers)
- [ ] **CIN-PLY-006** [FE] Next episode card with countdown, autoplay toggle, "Are you still watching?"
- [x] **CIN-PLY-007** [BE] Watch progress API (10s heartbeat, ≥ 90% watched) + cross-device sync *(phase 6: `/cinema/titles/{id}[/episodes/{eid}]/progress`, history, continue watching, `SR_CINEMA_UPDATE`; spec section 8)*
- [ ] **CIN-PLY-008** [FE] Resume prompt ("Resume from 42:10 / Start over")
- [x] **CIN-PLY-009** [BE] "Watching" presence emission (privacy-aware) *(phase 6: `SR_ACTIVITY_UPDATE` and `GET /cinema/me/friends-watching`, setting `cinema.share_watching`; folding it into `PRESENCE_UPDATE.activities` is a gateway follow-up)*
- [ ] **CIN-PLY-010** [FE] Signed URL refresh without playback interruption
- [ ] **CIN-PLY-011** [FE] Mini player (continue while navigating, also used by watch parties)

## CIN-MY — My list & history
- [ ] **CIN-MY-001** [FE] My list page (saved, continue watching, custom lists, history with pause/clear)
- [x] **CIN-MY-002** [BE] Lists API (shared with Library lists implementation where possible) *(phase 6: My list and custom lists in `sr-cinema-social`; the Library has no lists yet, so nothing was shared)*

## CIN-SOC — Social
- [x] **CIN-SOC-001** [BE] Ratings, reviews, episode comments (reuse LIB-SOC services generically) *(phase 6: ratings 1-10, reviews with spoiler flag and helpful votes, threaded comments with likes, reports and a moderation hook; no staff HTTP surface yet)*
- [x] **CIN-SOC-002** [BE] Trending (scoped) + recommendations *(phase 6: scoped trending and "because you watched" recommendations)*
- [ ] **CIN-SOC-003** [BE] Contribution credits for movies/episodes *(not built in phase 6: needs the cinema submissions pipeline, see cinema-media section 8.8)*

## CIN-DL — Patron downloads
- [ ] **CIN-DL-001** [BE] On-demand MP4 remux job of chosen rendition (cached 7 days), original download when allowed, signed single-user link, daily limit
- [ ] **CIN-DL-002** [FE] Download modal (rendition picker with sizes, progress of prep job)

## CIN-ADM — Admin
- [ ] **CIN-ADM-001** [FE] Catalog editor (title form, seasons/episodes tables, assets & renditions view, subtitles manager, artwork, markers editor with video scrubber, publish toggle, merge)
- [ ] **CIN-ADM-002** [FE] Transcode status per asset + re-run

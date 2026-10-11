# 14 — Cinema (`CIN`)

Phase 5. Spec: [cinema-media](../specs/cinema-media.md). Mockups: `cinema-discover.jpg`, `cinema-movies.jpg`.

## CIN-DATA — Catalog backend
- [ ] **CIN-DATA-001** [DB] video_titles, seasons, episodes, video_assets, renditions, audio_tracks, subtitle_tracks, trailers, markers, watch_progress, transcode_jobs, collections
- [ ] **CIN-DATA-002** [BE] Title/season/episode CRUD (publishers), publish/unpublish, merge duplicates, variants
- [ ] **CIN-DATA-003** [BE] Catalog API (discover rows, browse by kind with filters/sort, title detail, episodes, search)
- [ ] **CIN-DATA-004** [BE] Tantivy index for titles (title, original title, cast/crew text, tags)
- [ ] **CIN-DATA-005** [BE] Collections/rows API (curated + generated: continue watching, trending scoped, because you watched, new this week, friends are watching)
- [ ] **CIN-DATA-006** [BE] Age-restricted filtering

## CIN-ING — Ingest & transcoding
- [ ] **CIN-ING-001** [BE] Probe service (`ffmpeg-next`): streams, codecs, duration, HDR, interlace, languages, embedded subs
- [ ] **CIN-ING-002** [BE] Review proxy job (480p fast encode) for reviewers
- [ ] **CIN-ING-003** [BE] Transcode job planner (ladder ≤ source ≤ 1080p, audio tracks, subtitle extraction, sprites)
- [ ] **CIN-ING-004** [BE] Worker API: register/heartbeat, claim job, signed download of original, presigned multipart uploads for outputs, progress, complete/fail with logs
- [ ] **CIN-ING-005** [BE] `sr-transcoder` binary: NVENC detection, h264_nvenc / libx264 fallback, scale, HDR→SDR tonemap, deinterlace, closed aligned GOPs, fMP4 HLS 6s segments, AAC tracks, WebVTT subs, thumbnail sprite + VTT, segment index output
- [ ] **CIN-ING-006** [OPS] Transcoder CUDA container image + CPU image + "rent a GPU" guide
- [ ] **CIN-ING-007** [BE] Master/media playlist generation with signed segment URLs (6h), refresh endpoint
- [ ] **CIN-ING-008** [BE] ASS subtitle retention + WebVTT conversion
- [ ] **CIN-ING-009** [BE] Originals retention (cold bucket / Storage Box), proxy cleanup after publish
- [ ] **CIN-ING-010** [BE] Cinema `SubmissionKind` impls (title, episodes batch) with publish actions
- [ ] **CIN-ING-011** [BE] Re-transcode action (single asset / whole title / all — admin)

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
- [ ] **CIN-PLY-007** [BE] Watch progress API (10s heartbeat, ≥ 90% watched) + cross-device sync
- [ ] **CIN-PLY-008** [FE] Resume prompt ("Resume from 42:10 / Start over")
- [ ] **CIN-PLY-009** [BE] "Watching" presence emission (privacy-aware)
- [ ] **CIN-PLY-010** [FE] Signed URL refresh without playback interruption
- [ ] **CIN-PLY-011** [FE] Mini player (continue while navigating, also used by watch parties)

## CIN-MY — My list & history
- [ ] **CIN-MY-001** [FE] My list page (saved, continue watching, custom lists, history with pause/clear)
- [ ] **CIN-MY-002** [BE] Lists API (shared with Library lists implementation where possible)

## CIN-SOC — Social
- [ ] **CIN-SOC-001** [BE] Ratings, reviews, episode comments (reuse LIB-SOC services generically)
- [ ] **CIN-SOC-002** [BE] Trending (scoped) + recommendations
- [ ] **CIN-SOC-003** [BE] Contribution credits for movies/episodes

## CIN-DL — Patron downloads
- [ ] **CIN-DL-001** [BE] On-demand MP4 remux job of chosen rendition (cached 7 days), original download when allowed, signed single-user link, daily limit
- [ ] **CIN-DL-002** [FE] Download modal (rendition picker with sizes, progress of prep job)

## CIN-ADM — Admin
- [ ] **CIN-ADM-001** [FE] Catalog editor (title form, seasons/episodes tables, assets & renditions view, subtitles manager, artwork, markers editor with video scrubber, publish toggle, merge)
- [ ] **CIN-ADM-002** [FE] Transcode status per asset + re-run

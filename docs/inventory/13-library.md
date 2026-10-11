# 13 — Library (`LIB`)

Phase 4. Spec: [library-reader](../specs/library-reader.md). Mockup: `library-home.jpg`.

## LIB-DATA — Catalog backend
- [ ] **LIB-DATA-001** [DB] reading_series, reading_editions, volumes, chapters, pages, reading_progress, bookmarks, follows
- [ ] **LIB-DATA-002** [BE] Series/edition/chapter CRUD (publishers) + publish/unpublish + merge
- [ ] **LIB-DATA-003** [BE] Public catalog API (browse with filters/sort/pagination, series detail, chapter list, page manifest with sized URLs)
- [ ] **LIB-DATA-004** [BE] Tantivy index for series (titles, alt titles, authors, tags) + instant search endpoint
- [ ] **LIB-DATA-005** [BE] Age-restricted filtering everywhere
- [ ] **LIB-DATA-006** [BE] Latest updates feed (global + followed)

## LIB-ING — Ingest & conversion
- [ ] **LIB-ING-001** [BE] Image page pipeline: decode JPEG/PNG/WebP/AVIF/GIF → WebP (q85, lossless if smaller) + sized variants 720/1080/1600 + blurhash + dimensions + spread detection
- [ ] **LIB-ING-002** [BE] Archive readers: ZIP/CBZ, RAR/CBR, 7z/CB7 (natural sort, ignore junk files like `__MACOSX`, `Thumbs.db`)
- [ ] **LIB-ING-003** [BE] ComicInfo.xml parser → metadata
- [ ] **LIB-ING-004** [BE] PDF: keep original + rasterize pages (pdfium) for image mode
- [ ] **LIB-ING-005** [BE] EPUB: validate, extract metadata/cover, keep as-is
- [ ] **LIB-ING-006** [BE] series.json parser
- [ ] **LIB-ING-007** [BE] Auto-crop border detection data (optional per page)
- [ ] **LIB-ING-008** [BE] Library `SubmissionKind` impls (series, items) with publish actions
- [ ] **LIB-ING-009** [WASM] Client-side pre-checks (image validity, sizes) before upload

## LIB-UI — Section chrome
- [ ] **LIB-UI-001** [FE] Section top bar with "The library" active
- [ ] **LIB-UI-002** [FE] Left sidebar: logo + "The library", DISCOVER (Home, Browse, Latest updates, Requests), YOUR SPACE (My library w/ count, Reading history, Reading lists, Submissions, Upload), Help & shortcuts, Settings, tagline
- [ ] **LIB-UI-003** [FE] Top search with instant results dropdown (series, authors, genres, tags), keyboard nav
- [ ] **LIB-UI-004** [FE] Notifications bell (followed-series updates) + avatar menu
- [ ] **LIB-UI-005** [FE] Mobile layout (sidebar → drawer, bottom nav)

## LIB-HOME — Home
- [ ] **LIB-HOME-001** [FE] Heading "Find your next good story." + subline
- [ ] **LIB-HOME-002** [FE] The Spotlight carousel (backdrop art, eyebrow, title, ★ rating, status, genre chips, tagline, Start reading / View details, dots, caption)
- [ ] **LIB-HOME-003** [FE] Pick up where you left off (cards with progress, "Chapter 36 · Page 2 of 6") + Reading history link
- [ ] **LIB-HOME-004** [FE] Trending this week row (rank badge, meta, rating, genres) + scope switch Friends/People you know/Global + See rankings
- [ ] **LIB-HOME-005** [FE] Follow your curiosity genre chips + All genres
- [ ] **LIB-HOME-006** [FE] Fresh off the press list (thumb, series, "Chapter N · Language", release group, ▷) + All updates
- [ ] **LIB-HOME-007** [FE] A little inspiration reading-list card (fanned covers) + More lists
- [ ] **LIB-HOME-008** [FE] Finished stories, new beginnings row + Explore completed
- [ ] **LIB-HOME-009** [FE] Footer (About, Help, Image credits, Privacy, Terms)

## LIB-BRW — Browse, rankings, updates
- [ ] **LIB-BRW-001** [FE] Browse page: kind tabs, filter drawer (genres include/exclude, tags, status, language/edition, year, rating, age-restricted), sort, grid/list, virtualized infinite grid
- [ ] **LIB-BRW-002** [FE] Rankings page (scopes, periods: week/month/all-time)
- [ ] **LIB-BRW-003** [FE] Latest updates page (all / following toggle)

## LIB-SER — Series page
- [ ] **LIB-SER-001** [FE] Header: backdrop, cover, title, alt titles, authors/artists, status, year, rating with distribution + scope switch, genres/tags, synopsis expand
- [ ] **LIB-SER-002** [FE] Actions: Start/Continue reading, Follow (bell), Add to list ▾, Rate, Share to Lounge, Download (Patron)
- [ ] **LIB-SER-003** [FE] Edition selector
- [ ] **LIB-SER-004** [FE] Chapter list (volume groups, sort, read marks, group, date, mark read up to here, per-chapter download for Patrons)
- [ ] **LIB-SER-005** [FE] Tabs: Chapters, Reviews, Comments, Related, Contributors

## LIB-RDR — Reader
- [ ] **LIB-RDR-001** [FE] Reader shell (full-screen, auto-hiding top bar: back, series/chapter title, chapter select, settings, bookmarks, comments; bottom bar: page slider with thumbnails, prev/next chapter)
- [ ] **LIB-RDR-002** [FE] Paged mode RTL/LTR with tap zones (configurable, direction-aware)
- [ ] **LIB-RDR-003** [FE] Double-page mode (spread-aware pairing, cover offset toggle)
- [ ] **LIB-RDR-004** [FE] Vertical scroll / webtoon mode (gapless option, virtualized long strip)
- [ ] **LIB-RDR-005** [FE] Fit modes (width, height, original, smart) + zoom (pinch, wheel, double-tap) + pan
- [ ] **LIB-RDR-006** [FE] Keyboard shortcuts (arrows by direction, space, Home/End, F, D, M, B bookmark)
- [ ] **LIB-RDR-007** [FE] Image loader: sized-variant selection (viewport × DPR), preload next 3 + next chapter head, off-main-thread decode (`createImageBitmap`)
- [ ] **LIB-RDR-008** [FE] End-of-chapter card (next chapter, rate chapter, comments count)
- [ ] **LIB-RDR-009** [FE] Reader settings panel (mode, direction, background, gap, crop borders, smoothing, brightness) + per-series override
- [ ] **LIB-RDR-010** [FE] EPUB reader (foliate-js): font, size, line height, margins, theme, TOC, CFI progress
- [ ] **LIB-RDR-011** [FE] PDF reader (image mode default, pdf.js native option)
- [ ] **LIB-RDR-012** [BE] Progress API (page index / CFI, percent) + `SR_LIBRARY_UPDATE` cross-device sync + "continue where?" conflict prompt
- [ ] **LIB-RDR-013** [FE] Bookmarks (add with note, panel, jump)
- [ ] **LIB-RDR-014** [FE] Chapter comments drawer (spoilers, replies)
- [ ] **LIB-RDR-015** [FE] PWA offline cache for current + next chapter (Workbox runtime caching rules, quota handling)
- [ ] **LIB-RDR-016** [BE] Patron downloads: CBZ/PDF/EPUB export job, signed single-user link, daily limit
- [ ] **LIB-RDR-017** [BE] "Reading" presence activity emission (privacy-aware)

## LIB-SOC — Social & discovery
- [ ] **LIB-SOC-001** [BE] Ratings (1–10) per series/chapter + aggregates per scope (global, known, friends) with caching
- [ ] **LIB-SOC-002** [BE] "People you know" set computation + cache invalidation on membership changes
- [ ] **LIB-SOC-003** [BE] Reviews (text, spoiler flag, rating link, helpful votes) + comments (threaded)
- [ ] **LIB-SOC-004** [BE] Trending score job (global hourly) + on-demand scoped trending (10-min per-user cache)
- [ ] **LIB-SOC-005** [BE] Recommendations ("Because you read X": tag similarity + co-reading among known)
- [ ] **LIB-SOC-006** [BE] Lists (CRUD, order, visibility, share) + featured lists
- [ ] **LIB-SOC-007** [BE] Follow + new-chapter notifications (in-app, push, Lounge feed module)
- [ ] **LIB-SOC-008** [FE] My library page (status shelves: Reading, Plan to read, Completed, On hold, Dropped; unread counts; sort)
- [ ] **LIB-SOC-009** [FE] Reading history page (timeline, remove, pause)
- [ ] **LIB-SOC-010** [FE] Reading lists pages (mine, featured, list detail, editor)
- [ ] **LIB-SOC-011** [FE] Reviews tab UI (write/edit review modal, sort, helpful)

## LIB-ADM — Admin
- [ ] **LIB-ADM-001** [FE] Library catalog editor (series form, editions, chapters table, page reorder/replace/delete, re-convert, merge series)
- [ ] **LIB-ADM-002** [FE] Contributor credit adjustments (revoke for cause)

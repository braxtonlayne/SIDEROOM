# 13 — Library (`LIB`)

Phase 4. Spec: [library-reader](../specs/library-reader.md). Mockup: `library-home.jpg`.

## LIB-DATA — Catalog backend
- [x] **LIB-DATA-001** [DB] reading_series, reading_editions, volumes, chapters, pages, reading_progress, bookmarks, follows
- [x] **LIB-DATA-002** [BE] Series/edition/chapter CRUD (publishers) + publish/unpublish + merge
- [x] **LIB-DATA-003** [BE] Public catalog API (browse with filters/sort/pagination, series detail, chapter list, page manifest with sized URLs)
- [x] **LIB-DATA-004** [BE] Tantivy index for series (titles, alt titles, authors, tags) + instant search endpoint
- [x] **LIB-DATA-005** [BE] Age-restricted filtering everywhere
- [x] **LIB-DATA-006** [BE] Latest updates feed (global + followed)

## LIB-ING — Ingest & conversion
- [x] **LIB-ING-001** [BE] Image page pipeline: decode JPEG/PNG/WebP/GIF -> lossy WebP q85 (PNG sources also lossless, smaller kept; D-168) + sized variants 720/1080/1600 + blurhash + dimensions + spread detection. AVIF is rejected, not decoded (D-169)
- [x] **LIB-ING-002** [BE] Archive readers: ZIP/CBZ, 7z/CB7, RAR/CBR (behind `library.ingest.rar_enabled`; D-170) under one set of limits (natural sort, ignore junk files like `__MACOSX`, `Thumbs.db`)
- [x] **LIB-ING-003** [BE] ComicInfo.xml parser → metadata
- [ ] **LIB-ING-004** [BE] PDF: keep original + rasterize pages (pdfium) for image mode *(partial: `ingest::pdf::validate` accepts or refuses the original; rasterization is not implemented, see open-questions 2; the HTTP upload and job keep the original as the chapter source since phase 4)*
- [x] **LIB-ING-005** [BE] EPUB: validate, extract metadata/cover, keep as-is (`ingest::epub::inspect`)
- [x] **LIB-ING-006** [BE] series.json parser
- [x] **LIB-ING-007** [BE] Auto-crop border detection data (optional per page) *(phase 11: `ingest::crop`, stored in `reading_pages.crop` (0691) and in the page manifest; the reader does not apply it yet)*
- [x] **LIB-ING-008** [BE] Library `SubmissionKind` impls (series, items) with publish actions *(phase 5: `sr-library::submissions`, `library.series` and `library.items`; members build drafts and upload through the existing presigned-PUT + `library.ingest` path scoped to their submission, a curator with `REVIEW_READING` approves and the Library service publishes; see submissions-and-review spec section 9)*
- [ ] **LIB-ING-009** [WASM] Client-side pre-checks (image validity, sizes) before upload

## LIB-UI — Section chrome
- [x] **LIB-UI-001** [FE] Section top bar with "The library" active
- [x] **LIB-UI-002** [FE] Left sidebar: logo + "The library", DISCOVER (Home, Browse, Latest updates, Requests), YOUR SPACE (My library w/ count, Reading history, Reading lists, Submissions, Upload), Help & shortcuts, Settings, tagline
- [x] **LIB-UI-003** [FE] Top search with instant results dropdown (series, authors, genres, tags), keyboard nav
- [x] **LIB-UI-004** [FE] Notifications bell (followed-series updates) + avatar menu *(the bell lists updates from followed series, no unread count yet)*
- [x] **LIB-UI-005** [FE] Mobile layout (sidebar → drawer, bottom nav)

## LIB-HOME — Home
- [x] **LIB-HOME-001** [FE] Heading "Find your next good story." + subline
- [x] **LIB-HOME-002** [FE] The Spotlight carousel (backdrop art, eyebrow, title, ★ rating, status, genre chips, tagline, Start reading / View details, dots, caption) *(the spotlight is the most recently updated series until staff curation exists)*
- [x] **LIB-HOME-003** [FE] Pick up where you left off (cards with progress, "Chapter 36 · Page 2 of 6") + Reading history link
- [x] **LIB-HOME-004** [FE] Trending this week row (rank badge, meta, rating, genres) + scope switch Friends/People you know/Global + See rankings *(Global scope only, ranked by followers; Friends and People you know stay disabled until LIB-SOC-002/004)*
- [x] **LIB-HOME-005** [FE] Follow your curiosity genre chips + All genres
- [x] **LIB-HOME-006** [FE] Fresh off the press list (thumb, series, "Chapter N · Language", release group, ▷) + All updates
- [x] **LIB-HOME-007** [FE] A little inspiration reading-list card (fanned covers) + More lists *(static card with fanned covers until featured lists exist, LIB-SOC-006)*
- [x] **LIB-HOME-008** [FE] Finished stories, new beginnings row + Explore completed
- [x] **LIB-HOME-009** [FE] Footer (About, Help, Image credits, Privacy, Terms)

## LIB-BRW — Browse, rankings, updates
- [ ] **LIB-BRW-001** [FE] Browse page: kind tabs, filter drawer (genres include/exclude, tags, status, language/edition, year, rating, age-restricted), sort, grid/list, virtualized infinite grid *(partial: kind tabs, filter drawer, sort, grid/list and infinite loading work; no virtualized grid, genre and tag filters are free text)*
- [x] **LIB-BRW-002** [FE] Rankings page (scopes, periods: week/month/all-time) *(phase 11: `/library/rankings`, global scope only; Friends and People you know wait for a reading-privacy setting)*
- [x] **LIB-BRW-003** [FE] Latest updates page (all / following toggle)

## LIB-SER — Series page
- [x] **LIB-SER-001** [FE] Header: backdrop, cover, title, alt titles, authors/artists, status, year, rating with distribution + scope switch, genres/tags, synopsis expand *(partial: no rating or distribution yet)* *(phase 11: ratings with scope switch and distribution, `SeriesSocial.tsx`)*
- [ ] **LIB-SER-002** [FE] Actions: Start/Continue reading, Follow (bell), Add to list ▾, Rate, Share to Lounge, Download (Patron) *(partial: Start/Continue and Follow only)*
- [x] **LIB-SER-003** [FE] Edition selector
- [ ] **LIB-SER-004** [FE] Chapter list (volume groups, sort, read marks, group, date, mark read up to here, per-chapter download for Patrons) *(partial: volume groups, sort and read marks; no mark-read-up-to-here or downloads)*
- [ ] **LIB-SER-005** [FE] Tabs: Chapters, Reviews, Comments, Related, Contributors

## LIB-RDR — Reader
- [ ] **LIB-RDR-001** [FE] Reader shell (full-screen, auto-hiding top bar: back, series/chapter title, chapter select, settings, bookmarks, comments; bottom bar: page slider with thumbnails, prev/next chapter) *(partial: no comments button; thumbnails show while scrubbing the slider)*
- [x] **LIB-RDR-002** [FE] Paged mode RTL/LTR with tap zones (configurable, direction-aware)
- [x] **LIB-RDR-003** [FE] Double-page mode (spread-aware pairing, cover offset toggle)
- [ ] **LIB-RDR-004** [FE] Vertical scroll / webtoon mode (gapless option, virtualized long strip) *(partial: long strip mounts images near the viewport but is not virtualized)*
- [x] **LIB-RDR-005** [FE] Fit modes (width, height, original, smart) + zoom (pinch, wheel, double-tap) + pan
- [x] **LIB-RDR-006** [FE] Keyboard shortcuts (arrows by direction, space, Home/End, F, D, M, B bookmark)
- [x] **LIB-RDR-007** [FE] Image loader: sized-variant selection (viewport × DPR), preload next 3 + next chapter head, off-main-thread decode (`createImageBitmap`) *(uses Image.decode(), not createImageBitmap)*
- [ ] **LIB-RDR-008** [FE] End-of-chapter card (next chapter, rate chapter, comments count) *(partial: next chapter and back; rate and comments wait for LIB-SOC)*
- [x] **LIB-RDR-009** [FE] Reader settings panel (mode, direction, background, gap, crop borders, smoothing, brightness) + per-series override *(crop borders is disabled until LIB-ING-007; settings are kept on this device)*
- [ ] **LIB-RDR-010** [FE] EPUB reader (foliate-js): font, size, line height, margins, theme, TOC, CFI progress
- [ ] **LIB-RDR-011** [FE] PDF reader (image mode default, pdf.js native option)
- [x] **LIB-RDR-012** [BE] Progress API (page index / CFI, percent) + `SR_LIBRARY_UPDATE` cross-device sync + "continue where?" conflict prompt *(phase 4: `/library/chapters/{id}/progress`, read marks, history, `SR_LIBRARY_UPDATE`; see library-reader spec section 6)*
- [x] **LIB-RDR-013** [FE] Bookmarks (add with note, panel, jump) *(goes through the progress data layer, stored locally until the endpoints are wired)*
- [ ] **LIB-RDR-014** [FE] Chapter comments drawer (spoilers, replies)
- [ ] **LIB-RDR-015** [FE] PWA offline cache for current + next chapter (Workbox runtime caching rules, quota handling)
- [ ] **LIB-RDR-016** [BE] Patron downloads: CBZ/PDF/EPUB export job, signed single-user link, daily limit
- [ ] **LIB-RDR-017** [BE] "Reading" presence activity emission (privacy-aware)

## LIB-SOC — Social & discovery
- [x] **LIB-SOC-001** [BE] Ratings (1–10) per series/chapter + aggregates per scope (global, known, friends) with caching *(phase 11: `sr-library-social`, series and chapter ratings, aggregates computed live (no cache))*
- [ ] **LIB-SOC-002** [BE] "People you know" set computation + cache invalidation on membership changes
- [ ] **LIB-SOC-003** [BE] Reviews (text, spoiler flag, rating link, helpful votes) + comments (threaded)
- [ ] **LIB-SOC-004** [BE] Trending score job (global hourly) + on-demand scoped trending (10-min per-user cache)
- [ ] **LIB-SOC-005** [BE] Recommendations ("Because you read X": tag similarity + co-reading among known)
- [x] **LIB-SOC-006** [BE] Lists (CRUD, order, visibility, share) + featured lists *(phase 11: lists CRUD, order, visibility, staff feature endpoint)*
- [ ] **LIB-SOC-007** [BE] Follow + new-chapter notifications (in-app, push, Lounge feed module) *(partial, phase 4: publishing a chapter dispatches `SR_LIBRARY_UPDATE` `chapter_published` to followers with the bell on, and follow changes sync across devices; no stored notification list, push or Lounge feed yet)*
- [x] **LIB-SOC-008** [FE] My library page (status shelves: Reading, Plan to read, Completed, On hold, Dropped; unread counts; sort) *(phase 11: unread counts and sort)*
- [ ] **LIB-SOC-009** [FE] Reading history page (timeline, remove, pause)
- [x] **LIB-SOC-010** [FE] Reading lists pages (mine, featured, list detail, editor) *(phase 11: `Lists.tsx`; no staff UI for featuring yet)*
- [x] **LIB-SOC-011** [FE] Reviews tab UI (write/edit review modal, sort, helpful) *(phase 11: `ReviewsTab`)*

## LIB-ADM — Admin
- [ ] **LIB-ADM-001** [FE] Library catalog editor (series form, editions, chapters table, page reorder/replace/delete, re-convert, merge series)
- [ ] **LIB-ADM-002** [FE] Contributor credit adjustments (revoke for cause)

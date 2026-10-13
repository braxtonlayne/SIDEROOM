# Spec — The Library (reading)

Phase 4 · Theme `sideroom` · Mockup `assets/mockups/library-home.jpg` · Decisions: D-080, D-084, D-087–D-089, D-120, D-075.

## 1. Scope
Manga, manhwa, manhua, webtoons (vertical), Western comics, PDFs, EPUB novels and books. Global library (D-080). Multiple **editions** per series (language, official vs. fan release, group). Chapters can belong to volumes.

## 2. Information architecture

Top bar: `SIDE ROOM / The lounge · The cinema · **The library**`.

Left sidebar (per mockup):
- Header: book icon + "The library"
- **DISCOVER:** Home · Browse (renamed from "Browse manga," since scope is wider: tabs Manga / Manhwa / Comics / Books) · Latest updates · **Requests**
- **YOUR SPACE:** My library (count badge) · Reading history · Reading lists · **Submissions** (my uploads and their states) · **Upload** (visible to everyone: members submit, publishers publish)
- Footer: Help & shortcuts · Settings · tagline `MANGA, AT YOUR PACE.`

Top: search "Search manga, authors, and genres…" (instant results dropdown: series, authors, genres, tags), notifications bell (followed-series updates), avatar menu.

## 3. Pages

### 3.1 Home (`/library`)
| Block | Behavior |
|---|---|
| H1 "Find your next good story." + subline | Static, operator-editable |
| **The Spotlight** | Curated hero (staff, Collections & Spotlight). Artwork backdrop with gradient, eyebrow, title, rating ★, status, genre chips, tagline, `Start reading` / `View details`, carousel dots, "Handpicked for your next…" caption |
| **Pick up where you left off** | Cards with cover, title, "Chapter 36 · Page 2 of 6," progress bar. Links to Reading history |
| **Trending this week** | Poster row with rank badge on #1, type · chapter, rating, genres. **Scope switch: Friends · People you know · Global** (D-089). "See rankings" |
| **Follow your curiosity** | Genre chips → Browse filtered |
| **Fresh off the press** | Latest chapters: cover thumbnail, series, "Chapter 138 · English," release group label (replaces "Sample release"), ▷ read |
| **A little inspiration** | Featured reading list card ("For a slow Sunday") with a fanned cover stack |
| **Finished stories, new beginnings** | Completed series row |
| Footer | `SIDE ROOM / THE LIBRARY` · About · Help · Image credits · Privacy · Terms |

### 3.2 Browse (`/library/browse`)
Filters: kind, genre (include/exclude), tags, status, language/edition, year, rating, age-restricted (shown by default; "hidden" and "only" are options, and a member setting can make hidden the default; D-184), sort (trending, rating, newest, A–Z, recently updated). Grid/list toggle. Infinite scroll with a virtualized grid.

### 3.3 Series page (`/library/s/:id`)
Backdrop + cover, title, alt titles, authors/artists, status, year, rating (score + distribution; scope switch), genres/tags, synopsis (expand), **Follow** (bell), **Add to list**, **Start reading / Continue Ch. N**, share-to-Lounge. Edition selector. Chapter list (volume groups, sort asc/desc, read/unread marks, release group, date, "mark as read up to here," download icon for Patrons). Tabs: Chapters · Reviews · Comments · Related · Contributors (who uploaded, credited).

### 3.4 Reader (`/library/r/:chapterId[/:page]`)
Full-screen and distraction-free. The chrome shows on tap/hover.
- **Modes:** paged RTL (default for manga), paged LTR, **double page** (spread-aware; offset toggle for cover pages), **vertical scroll** (webtoon; continuous, gapless option), **long strip**, and EPUB reflowable mode (font, size, line height, theme, margins) via foliate-js. PDF: image mode, or native pdf.js mode.
- **Fit:** width, height, original, smart. **Zoom** (pinch, wheel, double-tap), pan.
- **Navigation:** tap zones (configurable, inverted for RTL), keyboard (←/→ honour direction, space, Home/End, `F` full-screen, `D` double page, `M` mode menu), page slider with thumbnails, chapter prev/next, auto-advance to the next chapter with an end-of-chapter card ("Next: Chapter 37 →," comments count, rate chapter).
- **Preloading:** next 3 pages and the first pages of the next chapter. Images are served as WebP in sized variants (w=720/1080/1600/original) chosen by viewport × DPR.
- **Progress:** saved per chapter (page index / EPUB CFI) on page turn (debounced 2s) and on unload. Synced across devices (`SR_LIBRARY_UPDATE`). "Continue on this device?" prompt when another device is further ahead.
- **Bookmarks:** per page, with optional note. Bookmarks panel.
- **Reader settings** (per series override): mode, direction, background color (black/grey/white/sepia), page gap, crop borders (auto-detect, WASM), image smoothing, brightness overlay.
- **Offline:** the PWA caches the current and next chapter automatically (free; cache only, not a file download). **Downloads** (CBZ/PDF/EPUB file export) are Patron-only (D-075).
- Comments drawer per chapter (spoiler-tag support).

### 3.5 My library, history, lists
- **My library:** followed/saved series with filters (Reading, Plan to read, Completed, On hold, Dropped), unread counts, sort.
- **Reading history:** timeline grouped by day, remove items, pause history.
- **Reading lists:** create, describe, order, set visibility (private / friends / everyone), share to Lounge, featured lists by staff.

### 3.6 Latest updates
Chronological feed of published chapters, filterable to followed series.

### 3.7 Upload & submissions
See the submissions spec. Library-specific metadata: kind, title, alt titles, authors, artists, language, edition label, direction, status, year, genres, tags, age-restricted, synopsis, cover, per-chapter number/title/volume/group.

## 4. Social & discovery logic
- **Ratings** 1–10 (shown as ★ /10 with one decimal). Aggregates are cached per scope: Global (all), Known (people you know, D-089), Friends.
- **Trending score** = Σ over reads in the last 7 days (unique readers × chapter completions, time-decayed). Computed per scope on demand for Known/Friends (cached 10 min per user) and globally by an hourly job.
- **Recommendations:** "Because you read X" via tag/genre similarity + co-reading among people you know. A simple, explainable model, no ML service.
- **Activity:** "Reading X" presence and the profile tab, respecting privacy (D-120).
- **Notifications:** followed-series new chapter (in-app, push, optional Lounge feed channel).

## 5. Acceptance criteria
- [ ] Page turns are < 100 ms perceived on a mid-range phone, with preloaded pages decoded off-main-thread.
- [ ] RTL, LTR, double-page, and vertical modes work with keyboard, touch, and mouse. Direction defaults from the series and can be overridden per series.
- [ ] Progress on phone shows on desktop within 5s.
- [ ] Trending differs correctly between Friends, Known, and Global for a seeded dataset (integration test).
- [ ] Non-Patrons have no download endpoint access (403 server-side).
- [ ] Age-restricted series are visible to everyone, but opening one (reader, page URLs) needs a confirmed 18+ answer: unconfirmed accounts get the "Are you 18 or older?" dialog once (403 `900011`), and the yes is remembered permanently (D-032, D-184).

## 6. HTTP API added in phase 4 (as built)

Ids are decimal strings, times are Unix milliseconds, errors are the usual `{code, message}` body. The OpenAPI document
(`packages/api-client/openapi.json`) is the exact contract; this section is the reading guide.

### 6.1 Chapter list pagination (keyset)

`GET /library/series/{id}/chapters` and `GET /admin/library/series/{id}/chapters` take `edition`, `limit` (1 to 500) and
`after` (opaque cursor) and `order=asc|desc` (default `asc`: edition, then chapter number, then id). Without `limit` every
chapter is returned, as before (`next_cursor` is then null). With `limit`, the response carries `next_cursor`; pass it back
as `after` for the next page. `editions` and `volumes` are repeated on every page (they are small).

### 6.2 Reading progress and cross-device sync (LIB-RDR-012)

| Route | Meaning |
|---|---|
| `GET /library/chapters/{id}/progress` | The caller's progress (`404` when there is none). |
| `PUT /library/chapters/{id}/progress` | Save progress. Body `{page_index, percent, cfi?, completed?, base_updated_at_ms?, force?}`. |
| `DELETE /library/chapters/{id}/progress` | Mark unread. |
| `POST /library/chapters/{id}/read` | Read marks. Body `{read: bool, up_to?: bool}`: mark this chapter read or unread, and with `up_to` every earlier chapter of the same edition ("mark read up to here"). Answers `{changed}`. |
| `GET /library/me/progress` | Reading history, newest first. `per_series=true` (default) keeps the latest chapter of each series ("pick up where you left off"), `false` lists every chapter. Keyset: `before`, `limit`. |

Opening a restricted series' chapter needs age confirmation here too (`900011`), exactly like the page manifest.

**Conflict prompt.** `PUT` answers `{progress, applied}`. When the client sends `base_updated_at_ms` (the `updated_at_ms` of the
progress it last saw, `0` for "none") and the stored progress is newer, nothing is written and the answer is
`{applied: false, progress: <server's>}`: another device read further. The client asks "continue where?" and either jumps to
the server position or repeats the `PUT` with `force: true`. Without `base_updated_at_ms` the write always applies. Every
chapter in `ChapterList` carries `progress.updated_at_ms` for use as the base.

**Gateway.** Every change dispatches `SR_LIBRARY_UPDATE` to the account's own sessions (see doc 05, section 4 for the
frame). Payload `type`: `progress` (the progress object), `progress_cleared`, `read_marks`, `bookmark_set`, `bookmark_removed`,
`follow`, `unfollow`, `upload` (an ingest finished or failed, for the uploader) and, to followers with the bell on,
`chapter_published`.

### 6.3 Bookmarks (LIB-RDR-013)

| Route | Meaning |
|---|---|
| `GET /library/chapters/{id}/bookmarks` | The caller's bookmarks in this chapter, by page. |
| `PUT /library/chapters/{id}/bookmarks` | Create or update the bookmark on `page_index` (body `{page_index, note?}`). One per page; 200 per chapter. |
| `DELETE /library/bookmarks/{bookmark_id}` | Remove one. |
| `GET /library/me/bookmarks` | All bookmarks with series and chapter titles, newest first (`before`, `limit`). |

### 6.4 Uploads and ingest (staff, `PUBLISH_READING`)

An upload targets an existing chapter (create it first: `POST /admin/library/editions/{id}/chapters` with the `format` it
will have). The file goes straight to object storage through a presigned URL, then a job converts it off the request path.

1. `POST /admin/library/chapters/{chapter_id}/uploads` with `{filename, size_bytes}`. The extension must fit the chapter's
   format: `images`: `.cbz .zip .cb7 .7z .cbr .rar`; `epub`: `.epub`; `pdf`: `.pdf`. The size is checked against the operator limit
   (`library.ingest.max_upload_mb`, default 512). Answers `201 {upload, upload_url, upload_method: "PUT", expires_in_secs, max_bytes}`.
2. `PUT` the bytes to `upload_url`.
3. `POST /admin/library/uploads/{upload_id}/complete` checks the object (present, within the limit) and queues the job. `202` with the upload.
4. Poll `GET /admin/library/uploads/{upload_id}` (or listen for `SR_LIBRARY_UPDATE` `upload`). `status`: `awaiting_upload`,
   `queued`, `running`, `done`, `failed` (`error` says why, naming the offending page or file), `expired` (never completed within a day).
   `done` carries `page_count`, `warnings` and `metadata` (title, number, volume, writers and so on read from ComicInfo or the EPUB, for the editor to offer).
5. `GET /admin/library/chapters/{chapter_id}/uploads` lists the recent uploads of a chapter.

For archives the job replaces the chapter's whole page list (the chapter keeps its published state; publish it as usual). For
EPUB and PDF the original is kept as the chapter's `source_key` and `GET /library/chapters/{id}/pages` returns `source_url` (a signed link) with no pages.
Only one upload per chapter may be in flight (`409` otherwise). Each upload and each outcome is written to the staff access log.
Operator settings (Admin, Limits, group `library`): `library.ingest.rar_enabled` (default on), `library.ingest.max_upload_mb`.

### 6.5 Storage garbage collection

Objects of replaced pages, deleted chapters, editions and series, replaced covers and replaced EPUB/PDF sources are queued
in the same transaction as the change (`reading_gc_keys`); the recurring job `library.gc` (hourly) deletes each one once no
page, cover or source references it, and expires abandoned upload slots (and their objects) after 24 hours.

### 6.6 Gaps

* `SubmissionKind` (`reading_series`, `reading_items`) and the review queue are not implemented: `sr-review` is an empty
  crate and no `submissions` table exists yet. Staff publish through the routes above (LIB-ING-008 stays open).
* Volume bundles (one archive holding several chapters) are not split automatically; upload one archive per chapter.
* PDF pages are not rasterized (owner decision pending, open-questions).

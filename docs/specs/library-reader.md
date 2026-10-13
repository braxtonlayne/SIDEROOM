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

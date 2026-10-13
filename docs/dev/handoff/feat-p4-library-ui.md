# Handoff: feat/p4-library-ui (Library web section; gate green)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p4-library-ui, branch feat/p4-library-ui (base integration/phase3). Frontend only (apps/web); no backend or token changes.

## Done
- **Routes** (`apps/web/src/library/routes.tsx`, one additive line in `router.tsx`): `/library` (home), `/library/browse`, `/library/updates`, `/library/s/:seriesId`, `/library/r/:chapterId/{-:page}` (reader, outside the section chrome), `/library/me`, `/history`, plus placeholder pages for lists, requests, submissions, upload. The old `Library` stub in `routes/pages.tsx` is gone.
- **Chrome (LIB-UI-001..005)**: section bar with "The library" active, sidebar per mockup (no "The board"), instant search combobox (series, authors, genres, tags; arrows, Home/End, Enter, Escape, `/` focuses it), bell (updates from followed series), avatar menu (includes the "Hide 18+ titles" switch that writes `library.hide_age_restricted` into the settings blob), phone layout (drawer + bottom nav), Help & shortcuts sheet.
- **Home (LIB-HOME-001..009)**, **Browse** (filters live in the URL: kind, status, genres, exclude, tags, language, years, age, sort, order, view; `?q=` shows instant-search results), **Latest updates** (all/following), **My library** (shelves), **Reading history**, **Series page** (header, 18+ badge, synopsis expand, edition select, Start/Continue, Follow, chapters by volume with read marks and sort).
- **Reader**: single page (RTL/LTR), double page (spread aware, cover offset), long strip (gap option); tap zones, keyboard (arrows by direction, Space, Home/End, F, D, M, B, +/-/0), zoom (wheel, double click, pinch) and pan, fit modes, brightness, background, smoothing, per-series override ("use for all series" switch), blurhash placeholders (own decoder, `reader/blurhash.ts`), sized-variant choice by viewport x DPR (`pickVariant`), preload of the next 3 pages and the first pages of the next chapter, signed-URL staleness refetch, end-of-chapter card, chapter select, bookmarks panel, URL kept in step with the page, progress saved (2 s debounce, on leaving, on pagehide).
- **Age gate** (`apps/web/src/lib/ageGate.tsx`): `<AgeGateProvider>`, `useAgeGate().ensureAdult(): Promise<boolean>`, `withAgeGate(fn, ensureAdult)`, `postAgeAnswer`. Wired to `POST /sr/age-gate {over_18}`; the reader fetches the manifest through `withAgeGate`, so a 403 with code 900011 opens "Are you 18 or older?", a yes refetches once, a no/dismiss returns to the previous page (or `/library`). Restricted titles are shown to everyone with an 18+ badge. **LNG-CHN-022 can reuse it**: mount `AgeGateProvider` above the Lounge shell and call `ensureAdult()` before opening a restricted channel.
- **Data layer** (`library/api.ts` fetchers for the existing routes; `library/progress.ts` for progress and bookmarks).
- Tests: 44 new (Vitest + MSW + Testing Library, axe on home, browse, series page and reader): `model.test.ts` (pure reader logic, blurhash, continue target), `library.test.tsx`, `reader.test.tsx` (incl. all four age-gate paths), `chrome.test.tsx`. Whole web suite: 39 files, 508 tests passing; typecheck, eslint, prettier, build clean.
- Inventory ticked: LIB-UI-001..005, LIB-HOME-001..009, LIB-BRW-003, LIB-SER-003, LIB-RDR-002/003/005/006/007/009/013. Partials are annotated in place (BRW-001, SER-001/002/004, RDR-001/004/008).

## Wiring the progress endpoints (p4-library-wiring)
Everything goes through `apps/web/src/library/progress.ts`. It has `httpProgress` (assumed shapes, listed at the top of the file) and `localProgress` (localStorage, active now). **Switch: set `USE_SERVER_PROGRESS = true`** and adjust `httpProgress` only if the real routes differ. Assumed:
- `PUT /library/chapters/{id}/progress {page_index, percent, completed}`, `GET` same path -> `ChapterProgress`
- `GET /library/me/progress?limit=` -> `{items: [{series, chapter}]}` with `chapter.progress` set, newest first (feeds "Pick up where you left off" and history)
- `GET|POST /library/chapters/{id}/bookmarks` (`{items:[{id,page_index,note,created_at_ms}]}` / `{page_index,note?}`), `DELETE /library/chapters/{id}/bookmarks/{bid}`
`Chapter.progress` from the chapter list (already in the generated schema) is preferred for read marks over the local map.

## Decisions
- Types are local (`library/types.ts`, mirroring the generated schema) so apps/web needs no new dependency or lockfile change; swap to `@sideroom/api-client` types when the web wiring lands it.
- The section has its own `QueryClient` and theme root (`section="library"`, user appearance but not the Lounge `userGlobal` theme, so the Side Room theme from the mockup is what you see). Anchors rendered by the UI kit (PosterCard, ArrowLink) are routed in-app by a click handler on the section root.
- The series id is not carried in the reader URL (TanStack would quote numeric strings in a search param, `?s=%22123%22`); the reader reads it from the manifest. Declining the age question goes back in history, else to `/library`.
- Spotlight = most recently updated series, Trending = most followed (D-173: no rating/trending sort yet), reading-list card is static. Scope switch shows only Global enabled.
- Reader prefs (mode, direction, fit, background, gap, smoothing, brightness, tap zones, cover offset) are stored in localStorage (`sr.library.reader.v1`), not the synced blob; no blob key is defined for them.
- `library.hide_age_restricted` is read from `GET /users/@me/settings` for the filter default and toggle; the server already applies it as the default `age` filter.

## Not done
- No Playwright spec: seeding a series with pages needs object-storage keys through the staff API, not cheap. Coverage is MSW only. No visual check in a browser either (no backend was started).
- Browse is infinite via "Load more" plus an IntersectionObserver sentinel, not a virtualized grid; genre/tag filters are free text (no facet endpoint).
- Series page: ratings, Add to list, Share, Download, Reviews/Comments/Related/Contributors tabs (disabled), mark read up to here. Rankings page (LIB-BRW-002), EPUB and PDF readers, comments drawer, offline cache, crop borders, "continue on this device?" prompt, `SR_LIBRARY_UPDATE` sync, cross-device progress.
- Reader uses `Image.decode()` for off-main-thread decode, not `createImageBitmap`.
- Footer links (About, Help, Image credits, Privacy, Terms) point at `/house-notes` and `/support`; there are no dedicated pages yet.

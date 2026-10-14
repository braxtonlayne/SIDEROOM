# Handoff: feat/p7-cinema-admin (Cinema publisher tools in the Admin section)

Worktree `SIDEROOM_wt/p7-cinema-admin`, branch `feat/p7-cinema-admin` (base `integration/phase6`). Frontend only (`apps/web/src/admin/cinema/`, a small `TitlePage.tsx` edit, one e2e spec). No backend, migration or OpenAPI change. Local commits only; nothing pushed.

## What exists
Three new Admin sections in a `CINEMA` nav group (registered in `admin/AdminApp.tsx`, additive):

| Section id | Visible with | Files |
|---|---|---|
| `cinema-catalog` | PUBLISH_MEDIA or DELETE_CONTENT | `CatalogSection.tsx` (list), `TitleEditor.tsx`, `ArtworkPicker.tsx`, `SeasonsPanel.tsx`, `ExtrasPanel.tsx`, `MarkersPanel.tsx`, `AssetPanel.tsx` |
| `cinema-collections` | MANAGE_COLLECTIONS | `CollectionsSection.tsx` |
| `cinema-transcoding` | MANAGE_TRANSCODE | `TranscodingSection.tsx`, `transcodeApi.ts` |

Shared: `cinema/api.ts` (typed `/admin/cinema/*` wrapper, spec section 8.5), `cinema/hooks.tsx` (`useLoad` that keeps the HTTP status, `useRun`, `LoadProblem`, list-field helpers).

- **Catalog list (CIN-ADM-001)**: debounced title search, kind/status/audience/sort filters, offset paging, poster thumb, publish/unpublish (API 409 message shown), Merge (search a target, confirm; needs both PUBLISH_MEDIA and DELETE_CONTENT), Delete (type-the-title confirm; DELETE_CONTENT only). A Media Curator sees neither Merge nor Delete.
- **Title editor**: tabs Details (every `TitleInput` field, genres/tags/languages/cast etc. as comma lists, age-restricted switch, IMDb/TMDB ids), Artwork (poster, backdrop, logo: pick, local preview, reservation route, presigned PUT via the existing `putFile`, then save the key), Seasons and episodes (series/anime; inline season and episode edit, still and season-poster upload, publish/unpublish, delete with confirm), Variants and trailers (CRUD), Skip markers, Video and transcoding.
- **Skip markers**: one intro/recap/credits window each, numeric seconds inputs (source of truth), a timeline strip, validation (end after start, within 24 h, both fields or none). "Load preview" lazily imports the player's `createEngine` and `getAssetPlayback` (so hls.js stays out of the admin chunk) into a `<video>`; once loaded, "Now" buttons copy the current time into a field and a playhead shows on the timeline. If the asset cannot be played the panel says so and the inputs still work.
- **Asset linking**: per film/episode asset-id field with Link/Unlink (`PUT .../asset`), plus a collapsible transcode status per asset with a Re-run action.
- **Collections**: list, create/edit (slug auto from name, display, surfaces, position, published), delete, ordered items with HTML5 drag and Up/Down buttons (keyboard path), add by title search, Save order / Revert.
- **Transcoding (CIN-ADM-002)**: tabs Queue (state filter, paged, logs modal, cancel, re-run), Workers (online, version, NVENC, drain switch), Worker tokens (create with one-time secret, revoke).
- **Download (CIN-DL-002)**: title page header Download now opens `DownloadModal` (films); for series the header button is disabled with a hint and each episode row has a Download button. The old `patron_download` gate (always false) is no longer used; a non-Patron gets the modal's own 403 copy.
- Permission-aware navigation and per-button hiding follow doc 07. 403 responses render a "You do not have access" state, not a generic error.

## Transcoding contract is ASSUMED
`p7-cinema-ingest-db` had no commits and no handoff when this was written, and spec section 4.4 documents only the worker-facing API. `cinema/transcodeApi.ts` header lists the routes the screens expect (`/admin/transcode/{workers,tokens,jobs,assets}`). A missing route (404/405/501 without an API error body) renders a "Not available yet" notice on every tab and in the per-asset status; nothing is faked. When the real routes land, change only that file (and its MSW fixtures in `admin/__tests__/cinema.test.tsx`). Reconcile names: job `state` values, `percent`/`stage`, token `secret`, the re-transcode body.

## Decisions
- **Artwork keys**: the API returns only signed `poster_url` etc., while `PUT /titles/{id}` replaces the keys (an omitted key clears the artwork). The editor recovers the key from the signed URL path (`/cdn/<key>`, `keyFromUrl`) and sends it back, so a metadata save keeps artwork. A `poster_key` field on staff `Title` would be cleaner; not added to avoid a backend change.
- Episodes: still upload is attached when the episode form is saved (key held in state).
- Delete takes no reason in the API, so deletes use a confirm (typed title for titles), not a reason dialog. Re-run transcode uses an optional reason.
- Section ids: `cinema-catalog`, `cinema-collections`, `cinema-transcoding` (navigation is local state inside a section, the route has no sub-paths).
- No subtitles manager: there is no subtitle catalog route (subtitles are renditions of the asset).
- Delete is hidden rather than disabled for curators; Merge likewise.

## Tests and gates
- `admin/__tests__/cinema.test.tsx`: 21 tests (MSW + Testing Library, axe on list, new-title form, markers, collections, transcoding): nav per role, list filters and publish conflict, delete confirm and merge, 403 state, create, artwork keys kept on save, artwork upload sequence with preview, non-image rejection, episode inline edit/add/publish conflict, markers validate/save, asset link and status, not-available state, collection reorder and save, transcoding tabs, token secret, helper units.
- `cinema/__tests__/cinema.test.tsx`: +1 test (Download opens the modal, non-Patron copy) and the stub assertion changed to enabled; `admin.test.tsx` Owner nav list extended with the three sections.
- Whole web suite: 85 files, 1068 tests pass. typecheck, eslint, prettier and build clean; bundle budget: admin 242.6 KB gzip (budget 600), cinema-player 221.3.
- `e2e/tests/admin-cinema.spec.ts`: Owner creates a draft, sees the publish refusal, links asset id `4242`, publishes, unpublishes from the list. Run on chromium against a real `sr-server` (build dir `p4-admin-ui-target`, throwaway database since dropped): 1 passed. The asset link accepts any id today (the asset tables belong to ingest), which is what makes this spec cheap; if ingest starts validating ids the spec needs a seeded asset. Locally `/readyz` fails without S3, poll `/healthz`.

Inventory ticked: CIN-ADM-001, CIN-ADM-002 (annotated as above), CIN-DL-002 annotation updated.

## Not done / follow-ups
- Real transcoding backend wiring (see above). Worker-token and queue screens are untested against a live server.
- Marker preview depends on the (assumed) `GET /cinema/assets/{id}/playback` route from the player agent; untested with real media.
- Contributors tab stays disabled (CIN-SOC-003 needs submissions).
- No subtitles manager, no bulk episode import, no artwork cropping, no unsaved-changes guard when switching tabs.
- Firefox/WebKit Playwright projects were not run.
- Markers for a series title (not episode) are not editable; the title-level PUT is used for films only.

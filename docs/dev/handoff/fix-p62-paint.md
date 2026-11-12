# Handoff: fix/p62-paint (SEC-AUD-144..145, D-636)

Branch `fix/p62-paint` (base `integration/phase61` at `3ac7c673`). Not pushed. No Rust, no migration, no route or response change.

## Done
- **e982158c SEC-AUD-144**: `Help` and `HelpOperators` (router.tsx, `HelpSkeleton` stays the fallback), `Browse` (library/routes.tsx) and `ShelfList` / `ShelfItemPage` (storage/shelves/routes.tsx) load lazily. `validateBrowseSearch`, `BrowseSearch`, `KINDS`, `STATUSES`, `AGES` and a `SORT_KEYS` list moved to `library/browseSearch.ts` (as `cinema/browseSearch.ts`); `Browse.tsx` imports them. The `SR_BUNDLE_REPORT=1` plugin in `vite.config.ts` writes `apps/web/bundle-report.json` (entry chunk, every module, largest first); `.gitignore` has the line. Test `library/browseSearch.test.ts` (validator; the route's `validateSearch` is that function, so it validates before `Browse` loads).
- **d54b911e SEC-AUD-145**: `check-bundle-budget.mjs` prints a first-paint row on every run (bytes, room to `singleChunkMaxGzipKB`) and fails below `firstPaintMinRoomKB` (new key, **8**); four markers keep the moved pages out of the entrance; `$comment` of `bundle-budget.json` rewritten.
- Final commit: this handoff and the local-only spec `e2e/tests/p62-paint-shots.spec.ts` (SR_P62_PAINT=1).

## Numbers (same commit, same build command, gzip -c of the `app-*.js` file)
- Before: **254,886** bytes (`app-jv4AGJwB.js`). After: **213,695** bytes (`app-DMUJevJm.js`). Saved 41,191 bytes. The check script (zlib gzip) shows 214,295 for the after file, room 41,705 bytes to 256,000.
- The target was reached with the three known moves: no step 3 moves were needed (240,000 target, 244,000 limit). `firstPaintMinRoomKB` is 8 (item 1 ended under 247,808).
- Rendered bytes that left the chunk (72 modules, 271,197 rendered bytes in total): Browse area (Browse, BrowseFilters, VirtualGrid, library ui/prefs/progress) about 61 KB, shelves area (ShelfItemPage, MetadataEditor, ShelfList, CbzReader, ShelfPlayer, review/api, uploadLedger) about 37 KB, Handbook about 14 KB; libraries only these pages used: @tanstack/virtual-core 49.7 KB, query-core queryObserver 17.5 KB, ui Select 13.9 KB, useMutation 6.2 KB; EpubReader 11 KB, PdfReader 5 KB. Per-move gzip bytes were not measured separately (one build for all three moves).
- Files outside my list: none changed.

## Top 25 modules still in the chunk (rendered bytes; chunk total 1,473,610)
1. 644593  NM:react-dom@19.3.0_react@19.3.0/node_modules/react-dom/cjs/react-dom-client.production.js
2. 39480  apps/web/src/voice/controller.ts
3. 38112  NM:@tanstack+router-core@1.171.35/node_modules/@tanstack/router-core/dist/esm/load-client.js
4. 36479  NM:@tanstack+router-core@1.171.35/node_modules/@tanstack/router-core/dist/esm/router.js
5. 26675  apps/web/src/voice/engine.ts
6. 23459  NM:@tanstack+query-core@5.104.1/node_modules/@tanstack/query-core/build/modern/queryClient.js
7. 21362  apps/web/src/store/reducer.ts
8. 19670  apps/web/src/store/store.ts
9. 19185  NM:react@19.3.0/node_modules/react/cjs/react.production.js
10. 18942  NM:@tanstack+router-core@1.171.35/node_modules/@tanstack/router-core/dist/esm/new-process-route-tree.js
11. 16824  apps/web/src/i18n/messages/en/entrance.ts
12. 15436  NM:@tanstack+query-core@5.104.1/node_modules/@tanstack/query-core/build/modern/query.js
13. 13672  apps/web/src/lounge/liveMap.ts
14. 13282  apps/web/src/settings/server/bridge/api.ts
15. 12530  apps/web/src/lounge/live.tsx
16. 11363  NM:@tanstack+history@1.162.5/node_modules/@tanstack/history/dist/esm/index.js
17. 11134  NM:scheduler@0.28.0/node_modules/scheduler/cjs/scheduler.production.js
18. 9760  packages/ui/src/tokens/themes.ts
19. 9740  apps/web/src/settings/user/prefs.ts
20. 9604  NM:@tanstack+react-router@1.17_282c79778dfc55fd93747bd5e9477de8/node_modules/@tanstack/react-router/dist/esm/link.js
21. 8777  apps/web/src/settings/server/api.ts
22. 8774  NM:@tanstack+router-core@1.171.35/node_modules/@tanstack/router-core/dist/esm/utils.js
23. 8764  NM:@tanstack+query-core@5.104.1/node_modules/@tanstack/query-core/build/modern/mutation.js
24. 8024  apps/web/src/voice/video.ts
25. 7918  NM:react-dom@19.3.0_react@19.3.0/node_modules/react-dom/cjs/react-dom.production.js
## Not moved and why
Not this wave by the brief: `voice/**` (controller 39.5 KB, engine 26.7 KB, video, soundboard, settings, playback, mic: about 115 KB together), `lounge/voice/VoiceBridge`, `store/**` (reducer 21.4 KB, store 19.7 KB, rest 6.3 KB), `gateway/client` (7.6 KB), `moderation/reports`, `cinema/player/model` (6.7 KB, MiniPlayerMount). Next wave's design input. Also in the chunk and probably not needed for first paint, not touched (the limit no longer needs it): `lounge/liveMap.ts` 13.7 KB, `lounge/live.tsx` 12.5 KB, `settings/server/bridge/api.ts` 13.3 KB (web builder's area), `settings/user/prefs.ts` 9.7 KB, `settings/server/api.ts` 8.8 KB, `tools/timers/timerCore.ts` 6.6 KB, `app/AppServerTemplate` / `AppServerInvite`, `pwa/signOutPurge` to `offlineCache`.

## Markers (check-bundle-budget.mjs, AREAS)
handbook `help.handbook.lounge.intro` (only in the HelpPage chunk and the catalog chunk en.rest, which is not an entrance file); browse `lib-filters`; shelves `shv-page`; epub `sr.library.epub.v1` (the EPUB reader lives in the PdfReader chunk). A marker found in no chunk also fails (stale marker).

## Tests
- Vitest: `library/browseSearch.test.ts` and `i18n/firstpaint.test.tsx`: 12 passed. I did not run the corePaths test or watch the new test fail without the change.
- One production build with SR_BUNDLE_REPORT=1; `check-bundle-budget.mjs` passes; `check-entrance-isolation.mjs` passes. Typecheck, eslint and prettier on the changed files were clean.
- Rules watched failing on a copy of dist (marker appended to the first-paint file): all four marker rows (handbook, browse, shelves, epub) showed FAIL; `firstPaintMinRoomKB` set to 60 showed the first-paint FAIL row. The real build passes with 8.

## Visual check (shots in `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p62-paint`)
- Sweep (`summary.md`: 6 routes, 24 captures; the only flag is a 404 console line on the Lounge control page, which this change does not touch): `help`, `help-operators`, `library-browse`, `library-shelves`, `cinema-shelves`, `lounge-channel` (control), desktop and phone, Lounge Light and the default (Editorial). Contact sheets in `sheets/` (000-002), all three viewed.
- Spec shots in `flow/`: help, help-operators, library-browse (`?kind=novel&sort=title&bogus=1`), library and cinema shelf list and item (item shows "That item is not on a shelf you can see", empty server), each in lounge-editorial, lounge-light and lounge-dark at desktop and phone (42 PNGs) plus `flow-browse-filter.png`. I opened: help phone light, library-browse desktop light, library shelf item phone dark, cinema shelf item desktop dark, flow-browse-filter. Not every one of the 42 was opened. Library and Cinema look the same in all three theme settings (their own theme), as before.
- Clicked through (spec, passed): Lounge, `/help` (full load), back, `/library/browse?kind=novel&bogus=1`, click Comic (URL gets `kind=comic`), reload (stays on Comic, loads directly), Library sidebar "Shelves", `/help` then reload. No page errors in any shot run. No wrong fallback seen.
- Observation, not a change: a filter click keeps unknown keys already in the URL (`bogus=1`); the router merges the raw parent search; same code path as before.
- The sweep's own Lounge Dark pass (`SR_SWEEP_DARK`) was not run.

## Proposed inventory rows
- SEC-AUD-144 (D-636): the Handbook, Library browse and shelf pages (with the EPUB reader and metadata editor) are lazy; the first-paint chunk fell from 254,886 to 213,695 bytes gzip (limit 256,000); `library/browseSearch.ts`. Done.
- SEC-AUD-145 (D-636): the bundle check prints the first-paint size and room, requires 8 KB (`firstPaintMinRoomKB`), keeps four moved areas out of the entrance by marker; `SR_BUNDLE_REPORT=1` writes the per-module report. Done.

## Proposed docs text
I did not grep `docs/` for `check-bundle-budget`. The `$comment` of `scripts/bundle-budget.json` holds the current statement. One-liner for a docs bundle section: "The first-paint chunk keeps at least 8 KB gzip of room under singleChunkMaxGzipKB; moved pages are guarded by markers; `SR_BUNDLE_REPORT=1 node scripts/build-web.mjs` writes a per-module report to apps/web/bundle-report.json."

## Owner questions
None.

## Cleanup
Stack stopped by PID, heavy lock released, `sr_p62_paint_ui` dropped, `p62-paint-target` and the copied binaries deleted. Untracked scratch files remain in the worktree root (p62-*, never committed).

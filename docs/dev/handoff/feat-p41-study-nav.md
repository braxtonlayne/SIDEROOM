# Handoff: feat/p41-study-nav (STU-NAV-001, wave 41 worktree 41-A)

## Resume here
(Updated at the end of the session; see "Status" below. If this section still says "in progress", the last commit on the branch is the safe point.)

## Done
- **Five Study switches** (`crates/sr-features/src/wave_modules.rs`), platform scope, client visible, OFF by default: `platform.external_metadata`, `platform.storefront_categories`, `platform.global_trending`, `platform.requests_board`, `platform.ingest_queues`. The 73-switch registry grew by five; `feature_registry.rs` and the web fixture (`registry.json`) are updated.
- **Hide, never delete.** No route, `route_matrix.tsv` row, spec or engine was removed. The `module_gate` layer (`crates/sr-api/src/module_gate.rs`) gained hidden rules that answer **404 `SR_NOT_FOUND`** while the switch is off (commander note): `/requests*`, `/cinema/trending`, `/library/rankings`, `/cinema/discover`, `/cinema/rows/{key}`, `/admin/review/queue`, `/admin/ingest-sources*`. No schema change, no migration (range 2000-2009 unused).
- **No outbound metadata calls.** `GatedLookupSwitch` (`crates/sr-server/src/lookup_wiring.rs`) puts `platform.external_metadata` in front of `review.lookup.enabled`; unit test `with_default_settings_no_provider_is_offered` proves nothing is offered by default even with every provider configured. Web side: Vitest `study.test.tsx` scans web sources for external metadata hosts (none).
- **Web.** `apps/web/src/study/` (flags, `HiddenWhenOff`, `RoomsNav`, `StudyAreas`, test helper). The section bars of the Lounge, Cinema and Library show the room trio **The Lounge, The Arcade (`/games`, still behind `platform.emulator`), The Study**; the Study's own bars show Media and Reading. Cinema nav (Movies/Series/Anime, Submit), avatar menu (uploads, Requests), Library nav (Requests, Submissions, Upload), Discover trending rail, Library home trending block, the rankings/requests/submissions/upload/browse pages (render not-found) and the admin Ingest sources section and Submissions review tab are hidden while off. `/cinema` opens on shelves when the storefront switch is off.
- **Editorial theme preset** `lounge-editorial` (packages/ui `themes.ts`, `sr-theme` BUILTIN_THEME_IDS, `tokens.generated.ts`, platform theme list), tokens only, and per the commander it is the **default for the Lounge** (`prefs.theme` default and `DEFAULT_THEME_BY_SECTION.lounge`); the other themes stay selectable. `lounge-dark` remains for system-dark sync.
- Tests that exercise the hidden surfaces call `switchStudySurfacesOn()` (`study/testing.ts`), so they say "off by default, on for this test". authz_matrix seeds the five switches on (the 404 rules are unit-tested in sr-api).
- Docs: D-393, doc 00 note, doc 10 theme row, doc 13 phase 4 and 5 notes.

## Not done / open
- Playwright smoke (exit-gate, lounge-shell): see Status.
- Handbook/entrance word list: the full web suite result is recorded under Status.
- The routes of the member upload pages in `route_matrix` are untouched; member-side upload/submission API routes (`/cinema/submissions`, `/library/submissions`) are NOT 404-gated (only the staff queue and ingest sources are), because the roadmap names "staff upload/ingest review queues". Owner question: gate the member submission API too?
- OpenAPI unchanged (no route added).

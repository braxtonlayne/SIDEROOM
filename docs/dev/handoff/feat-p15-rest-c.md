# Handoff: feat/p15-rest-c

Scope: the large slices rest-a left (Live TV imported guides, the in-chat watch party card, the canvas invite graph) and the independent K items of `docs/dev/answers-followups-p15.md` (K-01, K-02, K-03, K-08, K-09, K-10, K-14, plus K-19 folded into K-08).

Migrations used: **1000** (`tv_schedule_slots`, `tv_guide_imports`, `tv_favorites`, the `imported_guide` channel kind) and **1001** (`watch_party_cards`). 1002 to 1009 are free.

## Done (ticked)

K items
- **K-01** voice waveform: a consistent client waveform replaces the packet-size envelope (`sr_chat::voice_meta::declared_waveform_is_consistent`, `apply_voice_meta` in `sr-api/src/routes/messages/rich_create.rs`); doc 05.
- **K-02** holographic: a tertiary role colour is accepted only as the exact triple (`sr-chat/src/role_rules.rs`, route cases in `sr-api/tests/guilds.rs`, web constant test).
- **K-03** soundboard trimmer decodes at 48 kHz through an `OfflineAudioContext` (`soundTrim.ts`).
- **K-08 and K-19** `sr_patron` on every user object (`social::UserObject`, `messages::UserRef`, `guilds::UserRef`), computed in `PgChatRepo::users` (active or in grace, not `hide_badge`; the grace is `payment_config.grace_days`, read once at start by `sr_db::payments_repo::patron_grace_days` and bound with `PgChatRepo::with_patron_grace_days`); `GUILD_MEMBER_UPDATE` to shared servers when the flag can change (`MessagesDeps::refresh_member_objects`, called from `BusPaymentEvents::entitlement_user` and `PATCH /donations/me/preferences`); `Avatar ring` prop (`packages/ui`), used in the member list, message author, profile popout and voice tile; the preference reads "Hide my Patron badge and ring". DON-UI-005 and DON-ENT-006 ticked (every limit call site already reads `Entitlements::limits`; the audio bitrate cap is advisory in v1 by the voice spec).
- **K-09** `sr-payments/tests/esplora_live.rs` + `docs/dev/payments-live-test.md` (ran live against mempool.space signet, 3 of 3).
- **K-10** `GET /admin/system` carries `backups` and `disk` (`admin_jobs::FsProbe`, `AdminDeps::with_system_probe`); `backup.sh` touches `snapshot.ok`, `restore-drill.sh` touches `restore-drill.ok`; two cards on the System page. ADM-PLT-006 ticked.
- **K-14** doc and inventory lines (the ADM-GRAPH-003 "deferred" note was deliberately not written: the renderer was built).

Inventory
- **TV-ENG-001, TV-ENG-003, TV-ENG-007, TV-ADM-003, TV-ADM-004**: `imported_guide` channels. Migration 1000; `sr-tv`: `guide.rs` (CSV/JSON parse, fuzzy matching, report, confirm), `engine::resolve_slots` (resolver), `service/guide_ops.rs` (slots, imports, favorites, `revalidate_guides`), repo + Postgres impl; routes in `sr-api/src/routes/tv/guide.rs`; `userfeat.tick` revalidates every five minutes; `RepoTvCatalog::title_refs`. Web: `admin/cinema/tv/GuidePanel.tsx` (Guide tab: weekly grid with drag between days, form, title search, overlap check, import report with picks and confirm); channel favorites moved from the settings blob to `GET|PUT|DELETE /tv/favorites` (`cinema/tv/favourites.ts`, one-time move of the old list). Spec section 8.6.
- **WP-BE-008, WP-FE-008, WP-FE-010**: message type **200** `WatchParty`; `sr-watchparty` announces `Updated` kinds; `sr-server/src/party_cards.rs` (`PartyCards`) posts and rewrites the card (throttled to one edit per 15 s per party, ended card greyed, age-restricted title not named), table `watch_party_cards`; web `watchparty/PartyMessageCard.tsx` in `MessageItem`, one shared party state per channel (`useChannelParty.ts`). Spec watch-parties.md 7.4a, doc 05.
- **ADM-GRAPH-003**: `admin/sections/graph/` (`graphModel.ts`, `layout.ts` over `loadGraph()` from `@sideroom/wasm`, `GraphCanvas.tsx`), default view past 500 accounts.

## Decisions and notes
- TV-ADM-003 "weekly grid editor (manual channels)": the grid lives on `imported_guide` channels (staff can build one by hand); a `manual` channel still places programmes from the Now control tab. Say so if the owner wants a weekly template on manual channels as well.
- `next_program_early` behaves as a slate on imported-guide channels (a wall-clock grid has fixed starts).
- A generated slate or colour-bar block that is on air is replaced at once by a regeneration (so a first import takes effect without waiting for a long slate to end); a programme on air is always left to finish.
- The patron grace is read once at start (restart after changing `grace_days`).
- The watch-party card is always on for bound channels (the opt-in announcer module still posts its own text when enabled). Per-server opt-out is not built.
- Two strings in `admin-a-1.ts` (`admin.channelEditor.manual/guide.*`) were reworded in place because the "not available yet" copy was stale.
- `sr-db` embeds its migrations at compile time: after adding a migration `touch crates/sr-db/src/lib.rs` before running DB tests.

## Still open
- K-04, K-05, K-06, K-07, K-11, K-12, K-13 (left to others by the brief).
- DON-UI-001 (waits on the owner's copy), CIN-PLY-004 (K-13), TV-HLS-003/005, DON-PAY-009 (won't build in CI, D-257).

## Tests run
Gates (all green at the last commit):
- cargo: `fmt --check`; `clippy --all-targets -D warnings` on sr-chat, sr-db, sr-payments, sr-tv, sr-watchparty, sr-api, sr-server; `test` for sr-chat (lib), sr-tv (lib 73, Postgres `pg` 4), sr-watchparty (all, Postgres), sr-api (all: lib 602, api, guilds, lounge, openapi_names with `KNOWN_CLASHES` empty, route_matrix 5), sr-server (lib with ignored: 103 incl. `party_cards_pg_tests` and the probe test), sr-db (media_columns incl. the patron SQL with `grace_days = 5`, settings_repo, chat_repo, payments_repo, profile_be), sr-payments (`esplora_live` against signet, 3 of 3).
- web: `tsc --noEmit`, eslint on the touched trees, `prettier --check .`; vitest (maxWorkers 2) admin/cinema/watchparty/i18n/store/support/test (50 files, 571 tests) and lounge/settings/lib/voice/library/developers/api (152 files, 1440 tests); packages/ui (279), packages/wasm (14, with the graph wasm built), packages/api-client `mocks:check`; `vite build` and `scripts/check-bundle-budget.mjs` (admin 324 KB gz of 600, lounge 402 of 600, watch-party 250 of 600).
- OpenAPI regenerated (`packages/api-client/openapi.json`, `schema.d.ts`, MSW handlers, 844 operations) and the route matrix rows added (staff routes `staff`, favorites `self`).
- Not run: Playwright (shared ports), the full workspace `cargo test` (only the touched crates).

## Owner questions
- None new beyond the notes above.

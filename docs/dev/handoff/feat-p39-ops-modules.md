# Handoff, wave 39 ops-modules (OPS-FLG-001): per-guild and global switches for every new module

Branch `feat/p39-ops-modules`, based on `origin/integration/phase37` (a7d10bf0). Not pushed. Migrations: none needed
(1880 to 1889 unused; `profiles.access` is a catalog setting stored in the existing settings table).

## Registry audit (what the roadmap lists against what shipped)

| Roadmap id | Shipped id | Scopes | Default | State before this wave |
|---|---|---|---|---|
| `platform.byos` | same | platform, guild | on | registered (wave 28) |
| `lounge.scrapbook` | same | platform, guild | off, operator-gated | registered (wave 34) |
| `platform.public_profiles` | same | platform | off | registered (wave 34) |
| `platform.bridge` | same | platform | on | registered (wave 35) |
| `bridge.enabled` | same | guild | off, owner only, dies with `platform.bridge` | registered (wave 35) |
| `lounge.jukebox` | same | platform, guild | on | **registered here** |
| `lounge.listening_rooms` | same | platform, guild | on, dies with the jukebox | **registered here** |
| `platform.tools` + one sub-flag per tool | `platform.tools` and `platform.tools.<tool>` (31 tools; hyphens are underscores: `platform.tools.image_studio`) | platform | on | **registered here** |
| `platform.activities_datachannel` | same | platform | on | **registered here** |
| `platform.emulator` | same | platform | on | **registered here** |
| `platform.game_servers` | same | platform | on | **not here**: `feat/p39-gsv-page` registers it (see "Integrator" below) |
| `lounge.status_rings` | **`lounge.rings`** (mapped, not renamed) | platform | off | registered (wave 34, `sr-userfeat`) |
| `platform.watch_together_embeds` | same | platform | on | **registered here** |

The drawer's own `guide` has no sub-flag (it keeps the drawer explainable); every other tool of
`apps/web/src/tools/registry.ts` has one. A Vitest keeps the Rust list (`crates/sr-features/src/wave_modules.rs`,
between the `tool-flags:` markers) equal to the web registry, so a tool added later without a flag fails a test.

## What each kill switch stops

* **Route gate** (`crates/sr-api/src/module_gate.rs`, a route layer next to the BYOS gate; answers 403
  `SR_FEATURE_DISABLED`). It only refuses; a request with no credential passes to the handler so the anonymous answer stays 401.
  * `lounge.jukebox`: `/channels/{id}/jukebox*`, at platform scope and at the channel's server scope.
  * `lounge.listening_rooms`: `/channels/{id}/listening-room` (also needs the jukebox, both scopes).
  * `platform.tools` and the sub-flags: `GET /tools/fx` (currency), `/users/@me/tools/notes*` (notes),
    loot tables (`loot`, mine and the server's), `/channels/{id}/cards*` (`invite`), `PUT .../voice-timer`
    (any of pomodoro, stopwatch, countdown), `/channels/{id}/tool-rooms*` (any of tierlist, poll; the handler also checks
    the kind of a new room, so a switched-off poll tool refuses a poll room and still allows a tier list).
    Tools with no server part (JSON, regex, ...) are hidden by the web only.
  * `platform.emulator`: `GET /users/@me/games/roms`, and `/emu/cores/*` answers 404 (`web.rs`).
* **`platform.watch_together_embeds`**: `POST /watch-parties` and `PUT /watch-parties/{id}/media` refuse a body with an
  `external` link (403 `SR_FEATURE_DISABLED`, new `PartyError::FeatureDisabled`); Cinema and Live TV parties are untouched.
* **`platform.activities_datachannel`**: `sr_voice::datachannel::Gate` (an atomic shared by every shard, in `SfuConfig`):
  off, the SFU ignores the `sr-act` channel a member opens and drops what an open one sends (voice and media untouched);
  `ActivityService::set_datachannel_allowed(false)` strips the `datachannel` capability from every instance view, so a
  host never asks the call for a channel. `crates/sr-server/src/datachannel_gate.rs` copies the platform feature into both every 2 s
  (no restart).
* **Web** (from `GET /sr/features/flags`; only an explicit `false` hides): the drawer, gadgets and pin rail drop switched-off
  tools (`apps/web/src/tools/flags.ts`), the Games link and `/games` (an explanation page), and the voice jukebox strip.
  Guild-level jukebox off is enforced by the server; the strip is hidden by the platform flag only.

## `profiles.access` (D-382)

Admin setting, group `privacy`, values `walled` (default) and `opt_in_public`; any other word is refused. Public profile
pages are possible only when the platform feature `platform.public_profiles` is on **and** `profiles.access` is
`opt_in_public` (`public_possible` in `routes/packages/profiles.rs`); either alone keeps every profile inside the
instance. The roadmap's third value (public by default) is dropped, as the roadmap says. Behaviour change worth knowing:
an instance that had switched `platform.public_profiles` on must now also set `profiles.access`.

## Screens

Admin > Features and Guild Settings > Modules already render the registry from `GET /admin/features` and
`GET /guilds/{id}/sr/features`; nothing hand-made was added. `crates/sr-server/tests/feature_registry.rs` writes the
registry as `apps/web/src/admin/features/__fixtures__/registry.json` (refresh with `SR_UPDATE_FEATURE_CATALOG=1`, the test
fails when it is stale; the file is in `.prettierignore` because the Rust test pins its bytes) and
`apps/web/src/admin/features/__tests__/registryScreens.test.tsx` renders both screens from it and asserts every feature.

## Tests run

* Rust: `sr-features` (138), `sr-admin` (69), `sr-watchparty` (30 incl. DB), `sr-voice` (all; datachannel 8, two new gate tests),
  `sr-activities` (all incl. DB, the new capability test), `sr-api --lib` (804, incl. nine `module_gate` tests, one per switch),
  `sr-api --tests` (route_matrix, openapi_names, ...), `sr-server --lib` (tool rooms, datachannel gate), `sr-server web_gate` (17),
  `sr-db secret_columns` (2), `sr-server feature_registry` (7 + 1 ignored), `authz_matrix` (8 ok, including the new
  `module_kill_switches_stop_their_routes`, the one kill test per switch through the matrix client, and `profile_privacy_holds_over_http`
  with the walled default), `immutable_rules` (15 ok: refusals and logs hold with every flag killed, settings at min and max).
  Scratch database `sr_p39_ops_modules` (to be dropped).
* `cargo fmt --check`, `clippy -D warnings --all-targets` on sr-features, sr-admin, sr-voice, sr-activities, sr-watchparty, sr-api, sr-server.
* OpenAPI regenerated: no diff. Route matrix: no new routes, so no new row and no `authz_matrix` route row.
* Web: `tsc --noEmit`, eslint on the touched trees, prettier, `check-css-tokens`, vitest `src/emu src/tools src/admin src/settings src/lounge`
  (233 files, 1982 tests), `build-web.mjs`, `check-bundle-budget.mjs`, `check-entrance-isolation.mjs` all ok. `i18n-audit` reports 4 hard-coded strings,
  all in `tools/studio/StudioEditor.tsx` and `watchparty/StartPartyDialog.tsx` (not touched here). No Playwright (as instructed).

Kill tests of the features registered before this wave live where they were built: `platform.byos` (`storage_scopes_hold_over_http`,
`routes/storage/flags.rs` tests), `platform.bridge` (`sr-bridge/tests/db.rs`), `lounge.scrapbook` (`scrapbook_tests.rs`),
`platform.public_profiles` (`profile_privacy_holds_over_http`), `lounge.rings` (`sr-userfeat/tests/rings.rs`). The registry test pins all of them.

## Decisions and notes

* A gate before the handler means a killed route answers 403 `SR_FEATURE_DISABLED` to an authenticated caller who would otherwise
  have got 404 or 403 for another reason. It never turns a refusal into an admit.
* Harness change (ops-immutable owns this file next; expect a small merge): `Flip::KillFeatures` in `authz_matrix.rs` no longer PATCHes the 31
  tool sub-flags one by one (they follow `platform.tools`, which is killed first; a new check proves everything reads off), waits out a 429 on the
  admin routes (73 features exceeded the rate limit), and `SR_TEST_VOICE_PORT_BASE` moves the SFU port range (agents running the matrix at once
  collided on 52000). The matrix seed also sets `profiles.access = opt_in_public`.
* `tool_rooms::create` gained an `axum::extract::State<AppState>` parameter (the kind's switch); the Postgres test call site was updated.
* `lounge.rings` is the roadmap's `lounge.status_rings`; it is platform-only and default off, as wave 34 shipped it.

## Integrator

* After merging `feat/p39-gsv-page`: remove the `#[ignore]` of `game_servers_switch_is_registered_after_the_merge` in
  `crates/sr-server/tests/feature_registry.rs`, then refresh the fixture (`SR_UPDATE_FEATURE_CATALOG=1 cargo test -p sr-server --test feature_registry`). The registry test
  `game_servers_is_not_registered_on_this_branch` should be deleted at the same time. `platform.game_servers` needs no entry in `module_kill_switches_stop_their_routes`
  (it has no route); the fixture and the screens test cover it.
* If both gsv-page and ops-modules add a `p39-*` i18n fragment, the keys are unique (`emu.switchedOff.*` here).

## Still open

* A per-server switch for `lounge.listening_rooms`/`lounge.jukebox` hides nothing in the web strip (the server refuses the routes). Needs the guild module list in the voice view.
* The datachannel kill is proven at unit level (SFU loopback, activity service on Postgres, the sync task) and as a flag through the matrix client; a live two-browser check needs Playwright, which was not run.
* The member and operator handbook have no page for these switches yet (docs owners).

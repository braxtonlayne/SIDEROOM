# Handoff: feat/p39-gsv-page (GSV-PAGE-001)

Owner decision (2026-10-31, D-383): game servers stay deferred. The Game Servers page is **entirely cosmetic**: no `sr-api` route, no migration, no setting, no form, no working button, no backend. The one non-visual piece is the `platform.game_servers` feature registration.

## Done

- **Feature** `crates/sr-features/src/game_servers.rs`: `platform.game_servers`, platform scope only, default on, section "Games", `client_visible` (so `GET /sr/features/flags` carries it), no settings. Its description says it only shows a page. Registered in `crates/sr-server/src/app.rs` beside `platform.byos`. Unit test in the module.
- **Page** `apps/web/src/settings/server/gameservers/`: `GameServersSection.tsx` (under-construction hazard strip, "COMING SOON" heading, marquee, visitor counter that always shows seven zeros, two cards "Host it yourself (BYOC)" and "Managed servers", each a sentence plus a greyed `aria-hidden` mock-up of a server list, a status pill and a console strip, captioned "Preview only"), `GameServersTile.tsx`, `flag.ts`, `gameservers.css` (tokens only, logical properties only).
- **Marquee** reads `prefers-reduced-motion` through `useMediaQuery` (`data-motion="reduced"` stands it still and wraps the text) and the CSS also has a `@media (prefers-reduced-motion: reduce)` rule.
- **Nav**: a `SectionDef` `gameservers` in group APPS, `MANAGE_GUILD`, in `ServerSettings.tsx`. With the feature off (`platform.game_servers` explicit `false` in the client flags) the entry is gone and `/settings/server/:guild/gameservers` renders the not-found text (`app.router.notFound`, the same view the router uses).
- **Picker tile**: "Game servers: coming soon" in `ActivityLauncher.tsx`, under the same switch, in its own small component so the settings shell is not pulled into the launcher chunk.
- **Help**: a "Games" section in the member handbook (`Handbook.tsx`) with one line ("Game servers are coming later; ..."); the section-id list in `handbook.test.tsx` updated.
- **Strings**: fragment `apps/web/src/i18n/messages/en/p39-gsv-page-1.ts`.
- `scripts/check-css-tokens.mjs` TARGETS now include `settings/server/gameservers`, so the token rule is enforced on the page.

## Tests run

- `cargo test -j 2 -p sr-features` 137 passed; `cargo clippy -p sr-features --all-targets -D warnings` clean; `cargo fmt --check` clean (sr-features, sr-server).
- Vitest `apps/web/src/settings/server/__tests__/gameServers.test.tsx` (7 tests): both cards, no interactive element on the page, axe on the page and the tile, reduced motion on/off, nav entry rules, page opens with the feature on, not-found and no nav entry with it off, tile shown and hidden. Handbook and activities suites pass.
- Web typecheck, eslint, prettier clean. `node scripts/check-css-tokens.mjs` clean.
- `node scripts/i18n-audit.mjs` reports TOTAL 4, all in files this branch does not touch (`tools/studio/StudioEditor.tsx` 3, `watchparty/StartPartyDialog.tsx` 1); none from the new files.

## Still open / not done

- Full vitest (403 files) passes at --maxWorkers=2 once the wasm package is built (the first run had 9 file-load failures only because packages/wasm/dist was missing; rerun green). build-web, entrance isolation and bundle budget all ok.
- No Playwright (ports are not available this wave); no e2e spec written, since the page is inert.
- The registry test of OPS-FLG-001 (a separate agent) should list `platform.game_servers`; its id is exported as `sr_features::GAME_SERVERS_ID`.

## Decisions

- Switch semantics follow `platform.byos`: only an explicit `false` hides the page, so before the flags load it shows.
- The mock-up names ("Friday survival", "Co-op night") are generic on purpose; no real game is named.
- The route is the existing `:section` route; a hidden or unknown section still redirects to the first section, but `gameservers` with the feature off shows not-found as the roadmap asks.

## Owner questions

None.

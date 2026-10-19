# Handoff: feat/p17-jassub (M-08, K-13, CIN-PLY-004)

## Done
- `jassub@2.5.16` added to `apps/web`, pinned exactly (lockfile committed). Transitive: `abslink` 1.3.1, `lfa-ponyfill` 1.1.1, `throughput` 1.0.2, real `rvfc-polyfill` 1.0.8 (GPL-3.0, D-309). Integrity in `pnpm-lock.yaml`: jassub `sha512-n4vjXfra...mchQ9XVg==`, rvfc-polyfill `sha512-uA+0wwTk...R0uw7yElA==`.
- `apps/web/src/cinema/player/jassubLoader.ts`: lazy `import('jassub')` (own chunk, 5.4 KB gzip), worker and both `.wasm` files emitted as separate unminified files via `?worker&url` / `?url` (LGPL-2.1 section 6). `queryFonts: 'local'` plus JASSUB's bundled default font. Registered for every browser (no feature check, no stub, no override, no alias: D-309). `WatchRoute.tsx` calls `installJassub()`. `ass.ts` header rewritten.
- `scripts/check-licenses.mjs` + `web.yml` step "Licence allow-list" (`pnpm licenses list --json --prod | node scripts/check-licenses.mjs`): allow-list MIT, ISC, BSD-2/3, Apache-2.0, 0BSD, CC0-1.0, Unlicense, MPL-2.0; the jassub licence expression only for `jassub`, `GPL-3.0` only for `rvfc-polyfill`. Verified passing on the current tree and failing for an unknown AGPL / other GPL package.
- Doc 03 frontend row; CIN-PLY-004 (inventory 14), K-13 (p15 followups) and M-08 (p16 followups) ticked.
- Tests: `__tests__/jassubLoader.test.ts` (library stubbed: started with separate urls, disposed, WebVTT fallback on load failure and on constructor throw, install registers once and keeps an existing loader).

## Still open
- `THIRD_PARTY_NOTICES.md` does not exist on this branch (M-01 creates it). Integrator: add the sections below.
- Fonts: the track object has no fonts field, so `availableFonts` (K-13) still needs a server field.
- No Playwright case (cinema e2e not available in this wave; no servers).
- The "no chunk contains rvfc-polyfill" check from M-08 (c) is intentionally dropped (D-309).

## Notice text for THIRD_PARTY_NOTICES.md
- **JASSUB** 2.5.16, https://github.com/ThaUnknown/jassub, licence LGPL-2.1-or-later (package.json expression: `LGPL-2.1-or-later AND (FTL OR GPL-2.0-or-later) AND MIT AND MIT-Modern-Variant AND ISC AND NTP AND Zlib AND BSL-1.0`; the package's LICENSE file is the MIT text, "Copyright (c) 2021-2022 JASSUB contributors, (c) 2017-2021 JavascriptSubtitlesOctopus contributors"). Used unmodified. Relinking (LGPL-2.1 s.6): the web build serves JASSUB's worker (`assets/jassub-worker-*.js`) and WebAssembly modules (`assets/jassub-worker*.wasm`) as separate, unminified-by-us files that can be replaced; the source is the npm package `jassub@2.5.16` at the repository above.
- **libass** (ISC), **FreeType** (FreeType Licence FTL, or GPL-2.0-or-later), **fribidi** (LGPL-2.1-or-later), plus HarfBuzz/zlib-class components compiled into the wasm: include the licence texts shipped in the jassub package and upstream.
- **rvfc-polyfill** 1.0.8, GPL-3.0, imported by jassub and shipped as is in the JASSUB chunk (owner decision D-309).
- JASSUB default font (`dist/default.woff2`): Liberation Sans 2.00.5, OFL 1.1 (D-314). The notices were filled in on integration/phase17 (M-24).

## Tests run (all in the worktree)
- web `tsc --noEmit`: clean. `vite build` + `scripts/check-bundle-budget.mjs`: all budgets ok (cinema-player 245.7 of 600 KB). jassubLoader vitest: 4 passed.
- eslint, prettier and the full vitest (`--maxWorkers=2`): see the final report.

## Owner questions
- JASSUB's `package.json` licence expression says LGPL, its LICENSE file says MIT; we follow the stricter reading.

# Follow-up wave: fu-fable (branch fix/fu-fable)

## Done (commits)
- FU-ENT-4: `/app` reopens the last section. `state/session.ts` now persists `lastSection` (localStorage `sr.lastSection.v1`, per browser) and `rememberSection()`; `router.tsx` RootLayout writes it on in-app navigation. Only `/channels*`, `/cinema*`, `/study*`, `/games`, `/library*`, `/music*`; never settings, auth, errors or a query string. Vitest `state/session.test.ts`.
- FU-BETA-1: Caddy sends `/~*` to sr-server when the request carries an `sr_session` cookie; a refused cookie (404 from the server, which has `serve_public` off behind Caddy) or no cookie gets the static public profile shell (`handle_response` fallback). Done through a new `memberProxy` flag on the profile surface in `apps/web/public-surfaces.json`, rendered by `renderSurfacesBlock` in `security/csp.ts`; `Caddyfile.prod` re-rendered; csp.test.ts updated; Rust test added in `crates/sr-core/tests/deploy_files.rs` (written, NOT compiled: no cargo); `deploy/UPGRADE.md` release note.
- FU-STU-5: PS1 core starts without a BIOS on the built-in HLE BIOS (D-390). `CoreInfo.bios: 'none' | 'optional'`, `biosMode()`, the `bios` GameError and its copy are gone, `useEmuGame` returns `usesBios` and `hle`, EmuPage shows an info notice while running on HLE, BiosPanel shows a hint. Docs: `deploy/EMULATOR.md`, lawyer brief Q23. Tests: detect, emuPage.
- FU-ARC-5: Bitcoin/Lightning tip links of the Links block drawn as QR codes (160, ecc L) with link and text in the profile editor Inspector (`TipQrList`). Shared `apps/web/src/tips/QrBox.tsx`. Also fixes a latent bug: the old `QrBox` try/catch in `arcade/TipBadges.tsx` could not catch a throw from `QRCode`'s own render; it now pre-checks with `encodeQr`. Public profile unchanged. Vitest in `prf-pro.test.tsx`.
- FU-1..FU-12 rows in `docs/01-decisions-log.md` (one commit; P37-EMU-2 marked superseded); owner-checklist 4b/4c rows marked "Decided by Fable" and ticked; owner-only rows left as they were.

## Not done / left to the integrator
- `pnpm build`, `check-entrance-isolation`, `check-bundle-budget`: skipped on the commander's instruction (stale `packages/wasm/dist`).
- Rust test in deploy_files.rs not compiled or run.
- FU-STU-5 first step (confirm the pinned pcsx_rearmed starts with no BIOS file) could not be done: the core is a download and is not on the machine. Confirm when the cores are fetched; if it refuses, stop and report (do not patch the core).
- The Caddy `handle_response` block could not be validated (no caddy binary here): run `caddy validate` on `Caddyfile.prod`.
- The FU-12 log row's text ends "Reasons per row in `fu-fable-decisions.md`", a file outside the repo (kept verbatim).

## Routes shot (C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/fu-fable)
`app-door`, `emulator`, `profile-page-edit` (desktop/phone, light/dark) and `tipqr-desktop.png`, `tipqr-phone.png` (profile editor with a Links block holding bitcoin, lightning and https links). Read by eye: the QR blocks fit the 320 px side column and phone width. app-door lands in the Lounge, where a Vite overlay shows the stale-wasm import error (`dist/music/music.js`), an environment problem, not this change. The emulator shot has no ROMs, so the HLE notice and PS1 hint are covered by vitest only.

## Tests run
tsc clean; eslint on changed files clean; prettier clean; vitest: state, security, emu, profilepage, arcade pass; `check-css-tokens`, `i18n-audit` clean; `ops files` check ok; `playwright test --list` loads 888 tests. Stack stopped, DB `sr_fu_fable` dropped.

## Owner questions (defaults taken)
1. `lastSection` is per browser, not per account (a shared browser reopens the previous account's section). OK, or key it by account id?
2. `/games/emulator` and `/games/*` subpaths are not remembered, only `/games`.
3. Caddy proxies `/~*` only for requests with a session cookie, so anonymous profile views never touch sr-server. OK?
4. The QR blocks are in the editor only (the Links block inspector); the signed-in profile view uses `packages/profile-blocks` (out of scope) and still shows text only. Add QR to the view page later?
5. A delisted-by-shelf PS1 HLE notice is shown only while the game runs; the BIOS panel always shows the HLE hint when no BIOS is loaded.

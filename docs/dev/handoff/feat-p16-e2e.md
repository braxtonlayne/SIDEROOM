# feat/p16-e2e handoff

## Done
- **ENT-UI-026** `e2e/tests/entrance-journey.spec.ts`: one UI-driven chain: invite (sr-cli) -> register -> verify email -> enable authenticator app in Settings (TOTP secret read from the "Manual entry key" shown in the wizard, code computed with `support/totp.ts`) -> log out via the UI -> password + TOTP login -> add a passkey (CDP virtual authenticator) -> log out -> passkey-only login. Chromium only.
  - Mail sink: the e2e stack uses the log-only mailer, which logs the body. `e2e/support/mailLog.ts` reads the server log (`server.log` in the repo root, which `deploy/compose/e2e-run.sh` writes, or `SR_E2E_SERVER_LOG`) and extracts the token. No server change. CI needs no change (it already starts the server with output in `server.log`).
- **XC-TEST-005** `e2e/tests/visual.spec.ts`: 9 `toHaveScreenshot` baselines (Entrance: landing, login, forgot, no-invitation, house-notes; Cinema: discover, movies; Library: home, browse), 1280x800, UTC/en-US, animations disabled, caret hidden, 1% pixel ratio. The Cinema pages show one fixed published title seeded through the admin API (idempotent).
  - CI choice: opt-in with `SR_E2E_VISUAL=1`, chromium only. Pixel output depends on the OS fonts and rasteriser, and Playwright names baselines per platform (`*-chromium-win32.png`), so the committed baselines are Windows ones; a default CI run (linux) skips the file instead of failing on missing baselines. To enable on CI: run once with `SR_E2E_VISUAL=1 ... --update-snapshots` on the linux runner, commit the `-linux` PNGs, then set the variable in the workflow.
  - Library has no seed data (ingest needs files), so Library snapshots show empty states; masks were not needed because no dynamic text is on screen in the captured viewport.

## Still open
- Comparison "vs mockups" is by eye only; baselines are the app's own rendering. Library with real series covers needs an ingest fixture.
- Visual job not wired into the CI workflow (needs linux baselines, see above).

## Tests run
- entrance-journey 4 runs green (1 + `--repeat-each=3`); visual 3 repeats green plus a run without the env var (9 skipped); `tsc --noEmit` in e2e clean. Chromium project only, scratch DB `sr_e2e_p16e` (dropped), Vite + sr-server started by me and stopped by PID.

## Notes
- Stack on this host: `cargo build -p sr-server -p sr-cli -j 2`; `SR_CLI_BIN` pointed at the built `sr-cli.exe`; `DATABASE_URL` read by `support/cli.ts`; Vite started manually (`pnpm` is not on PATH for Playwright's webServer) with `VITE_SR_E2E=1`.
- No migrations, no new dependencies.

## Owner questions
- Should CI run the visual specs (needs linux baselines committed)?

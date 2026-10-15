# Open questions

Only questions that still need a human are listed here. Every other question was decided, and each answer is in [open-question-answers.md](open-question-answers.md) (the Phase 2 round is the section "Phase 2 answers (2026-10-13)"; its entries are numbered `OQ-n` with the numbers this file used before the sweep). Adopted answers are D-157 to D-183 (Phase 2 round) and D-185 to D-216 (Phase 8 to 11 round) in [01-decisions-log.md](01-decisions-log.md), and the implementation work they imply is listed in [dev/answers-followups.md](dev/answers-followups.md) and [dev/answers-followups-p12.md](dev/answers-followups-p12.md). Paths are relative to the repo root.

Remove an entry when it is decided, and record the decision in [01-decisions-log.md](01-decisions-log.md).

## Owner decisions

For each of these, the recommended answer was adopted as the default and is being implemented. The owner confirms it or overrides it.

1. **UnRAR licence for RAR/CBR reading.** (D-170) The `unrar` crate vendors RARLAB's UnRAR source (crate `unrar_sys`), whose manifest says `MIT OR Apache-2.0` but whose actual licence (`vendor/unrar/license.txt` in the crate) is the RARLAB freeware licence: free to use "in any software to handle RAR archives without limitations", not usable to build a RAR-compatible archiver, and modified UnRAR source distributed with other software must carry its licence paragraph. SIDEROOM only decompresses, so it fits a self-hosted deployment and RAR reading is implemented and on by default. The default adopted: keep it on behind `library.ingest.rar_enabled`, and include the UnRAR licence text in release notices. `cargo deny` cannot see this (it trusts the manifest). The owner confirms, or switches RAR off for good by deleting the `unrar` dependency (7z and ZIP are unaffected). — [crates/sr-library/Cargo.toml](../crates/sr-library/Cargo.toml)
2. **PDF page rasterization (LIB-ING-004).** ADR-0005 names `pdfium-render`, which needs a native pdfium library shipped beside the binary (no pure-Rust rasterizer is practical). Not implemented: PDFs are validated and kept as originals only. Default: ship the original for download and defer image-mode PDFs until the owner approves bundling a pdfium build (a download, so not done by agents). Answered as P11-LIB-2 in the Phase 8 to 11 round: PDFs are read with `pdf.js` in the browser and the cover is rendered client-side, so no server rasterizer is needed unless the owner approves the bundle.

### From the Phase 8 to 11 round (2026-10-15)

Full text in [open-question-answers.md](open-question-answers.md), section "Phase 8 to 11 handoff questions"; adopted answers are D-185 to D-216, follow-ups in [dev/answers-followups-p12.md](dev/answers-followups-p12.md). Each item below applies its default until the owner answers.

3. **P8-DON-4 Esplora endpoint and price sources** (external services). Default: self-hosted `electrs` on the box; two public price APIs, median.
4. **P11-MSG-1 Default sticker packs** (artwork). Default: none ship; the `sticker_packs` plumbing is built for reviewed packs later.
5. **P11-SHL-3 Avatar decoration artwork** (artwork). Default: the four CSS effects only; no decorations.
6. **P11-CLI-1 Server-tag badge glyphs** (artwork). Default: the 16 Unicode glyphs.
7. **P11-USB-2 Discord OAuth app for account linking** (external account). Default: reuse the `[discord]` app; the owner registers the second redirect URI `<public_url>/api/v1/connections/discord/callback`.
8. **P11-LIB-2 pdfium bundle** (download). Default: client-side `pdf.js`, no server rasterizer (item 2 above).
9. **MOD-4 Copyright process** (legal). Default: staff-entered notices, staff-recorded counter-notices, strike review at 3 upheld takedowns in 12 months.
10. **MOD-5 Minimum age and underage handling** (legal). Default: minimum registration age 13 by self-declaration; under 18 = reset age confirmation and restrict; under 13 = suspend.
11. **MOD-6 Data-subject jurisdiction and deadline** (legal). Default: 30-day due date (`compliance.data_request_deadline_days`); deletion copy states what D-124 retains.

Earlier confirmations (age gate, font, door image, token calibration) are recorded in [open-question-answers.md](open-question-answers.md).

## Human actions (not decisions)

1. **pnpm is not installed on the agent machines.** (OQ-11) Install it with `corepack enable && corepack prepare pnpm@10.12.1 --activate`. Until then agents use `npx pnpm@10.12.1` with `npm_config_manage_package_manager_versions=false` and never npm for lockfile changes. — [pnpm-workspace.yaml](../pnpm-workspace.yaml)
2. **Markdown corpus review against Discord.** (OQ-15) Someone with a Discord account reviews a stratified sample of 100 corpus cases after the parser quirk decisions (Q31, Q89, Q90) are in; mismatches become hand-written expectations. — [tests/corpus](../crates/sr-markdown/tests/corpus)
3. **First green CI run.** (OQ-16) `release.yml`, `openapi.yml`, `storage.yml` and `e2e.yml` have never run on GitHub; pushing is the owner's call. FND-CI-004 stays unticked until `e2e.yml` passes. The Postgres-backed `#[ignore]` tests get their CI step in the answers follow-ups. — [.github/workflows](../.github/workflows)
4. **Cross-cutting work that needs the owner (XC).** (1) Turn on "Require review from Code Owners" and make the new `Repo checks`, `Web` and `Release dry run` jobs required in the branch protection rules; `.github/CODEOWNERS` is only advisory until then. (2) Run `docker compose -f deploy/compose/prod.yml config` and a first real install on a clean host: `prod.yml`, the systemd units and `Caddyfile.prod` were written and linted but never run (no Docker on the agent machines). (3) Decide the voice and activities origins for the Content-Security-Policy (`apps/web/src/security/csp.ts` and `Caddyfile.prod` must change together; `connect-src` and `frame-src` are `'self'` today). (4) Web Push (XC-PWA-004) needs a VAPID key pair and a push sender; generate the keys on the server host and keep them out of the repo. (5) `pnpm audit` and `shellcheck` run for the first time in GitHub Actions; fix or waive what they report. (6) Entrance muted text uses a lighter local colour than `--sr-text-3` to pass WCAG AA contrast (see `components/entrance.css`); confirm the look, or raise `--sr-text-3` itself, which is a token calibration decision.

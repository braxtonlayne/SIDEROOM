# Open questions

Only questions that still need a human are listed here. Every other question was decided, and each answer is in [open-question-answers.md](open-question-answers.md) (the Phase 2 round is the section "Phase 2 answers (2026-10-13)"; its entries are numbered `OQ-n` with the numbers this file used before the sweep). Adopted answers are D-157 to D-183 in [01-decisions-log.md](01-decisions-log.md), and the implementation work they imply is listed in [dev/answers-followups.md](dev/answers-followups.md). Paths are relative to the repo root.

Remove an entry when it is decided, and record the decision in [01-decisions-log.md](01-decisions-log.md).

## Owner decisions

For each of these, the recommended answer was adopted as the default and is being implemented. The owner confirms it or overrides it.

1. **UnRAR licence for RAR/CBR reading.** (D-170) The `unrar` crate vendors RARLAB's UnRAR source (crate `unrar_sys`), whose manifest says `MIT OR Apache-2.0` but whose actual licence (`vendor/unrar/license.txt` in the crate) is the RARLAB freeware licence: free to use "in any software to handle RAR archives without limitations", not usable to build a RAR-compatible archiver, and modified UnRAR source distributed with other software must carry its licence paragraph. SIDEROOM only decompresses, so it fits a self-hosted deployment and RAR reading is implemented and on by default. The default adopted: keep it on behind `library.ingest.rar_enabled`, and include the UnRAR licence text in release notices. `cargo deny` cannot see this (it trusts the manifest). The owner confirms, or switches RAR off for good by deleting the `unrar` dependency (7z and ZIP are unaffected). — [crates/sr-library/Cargo.toml](../crates/sr-library/Cargo.toml)
2. **PDF page rasterization (LIB-ING-004).** ADR-0005 names `pdfium-render`, which needs a native pdfium library shipped beside the binary (no pure-Rust rasterizer is practical). Not implemented: PDFs are validated and kept as originals only. Default: ship the original for download and defer image-mode PDFs until the owner approves bundling a pdfium build (a download, so not done by agents).

Earlier confirmations (age gate, font, door image, token calibration) are recorded in [open-question-answers.md](open-question-answers.md).

## Human actions (not decisions)

1. **pnpm is not installed on the agent machines.** (OQ-11) Install it with `corepack enable && corepack prepare pnpm@10.12.1 --activate`. Until then agents use `npx pnpm@10.12.1` with `npm_config_manage_package_manager_versions=false` and never npm for lockfile changes. — [pnpm-workspace.yaml](../pnpm-workspace.yaml)
2. **Markdown corpus review against Discord.** (OQ-15) Someone with a Discord account reviews a stratified sample of 100 corpus cases after the parser quirk decisions (Q31, Q89, Q90) are in; mismatches become hand-written expectations. — [tests/corpus](../crates/sr-markdown/tests/corpus)
3. **First green CI run.** (OQ-16) `release.yml`, `openapi.yml`, `storage.yml` and `e2e.yml` have never run on GitHub; pushing is the owner's call. FND-CI-004 stays unticked until `e2e.yml` passes. The Postgres-backed `#[ignore]` tests get their CI step in the answers follow-ups. — [.github/workflows](../.github/workflows)

# Open questions

Only questions that still need a human are listed here. Every other question was decided, and each answer is in [open-question-answers.md](open-question-answers.md) (the Phase 2 round is the section "Phase 2 answers (2026-10-13)"; its entries are numbered `OQ-n` with the numbers this file used before the sweep). Adopted answers are D-157 to D-183 in [01-decisions-log.md](01-decisions-log.md), and the implementation work they imply is listed in [dev/answers-followups.md](dev/answers-followups.md). Paths are relative to the repo root.

Remove an entry when it is decided, and record the decision in [01-decisions-log.md](01-decisions-log.md).

## Owner decisions

For each of these, the recommended answer was adopted as the default and is being implemented. The owner confirms it or overrides it.

1. **Lounge sans font family.** (OQ-12) The Side Room families are fixed by doc 10 §2 (Newsreader, Hanken Grotesk, IBM Plex Mono, all OFL). Recommendation: **Inter** (OFL, variable WOFF2) as the stand-in for Discord's gg sans. The owner confirms the family and adds the WOFF2 files plus `OFL.txt` under `apps/web/public/fonts/<family>/` for all four families; `fonts.ts` already expects those paths. Downloading fonts is an owner action under the working agreements. — [fonts.ts](../packages/ui/src/theme/fonts.ts)
2. **Door image.** (OQ-14) Supply `apps/web/public/entrance/door.jpg` (recommended at least 1704 px wide, at most 400 KiB, EXIF stripped). The Admin → Entrance upload replaces the file later; until then `Door.tsx` keeps its placeholder. — [Door.tsx](../apps/web/src/components/Door.tsx)
3. **Token calibration against the mockups.** (OQ-13) A visual pass at 1440 and 1704 px against [docs/assets/mockups](assets/mockups/README.md) and under 900 px, landed as token-only PRs. Recommendation: do it after the fonts are in, since type metrics move the spacing. — [packages/ui/src/theme](../packages/ui/src/theme)

## Human actions (not decisions)

4. **pnpm is not installed on the agent machines.** (OQ-11) Install it with `corepack enable && corepack prepare pnpm@10.12.1 --activate`. Until then agents use `npx pnpm@10.12.1` with `npm_config_manage_package_manager_versions=false` and never npm for lockfile changes. — [pnpm-workspace.yaml](../pnpm-workspace.yaml)
5. **Markdown corpus review against Discord.** (OQ-15) Someone with a Discord account reviews a stratified sample of 100 corpus cases after the parser quirk decisions (Q31, Q89, Q90) are in; mismatches become hand-written expectations. — [tests/corpus](../crates/sr-markdown/tests/corpus)
6. **First green CI run.** (OQ-16) `release.yml`, `openapi.yml`, `storage.yml` and `e2e.yml` have never run on GitHub; pushing is the owner's call. FND-CI-004 stays unticked until `e2e.yml` passes. The Postgres-backed `#[ignore]` tests get their CI step in the answers follow-ups. — [.github/workflows](../.github/workflows)

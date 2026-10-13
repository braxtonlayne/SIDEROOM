# Open questions

Only questions that still need a human are listed here. Every other question was decided, and each answer is in [open-question-answers.md](open-question-answers.md) (the Phase 2 round is the section "Phase 2 answers (2026-10-13)"; its entries are numbered `OQ-n` with the numbers this file used before the sweep). Adopted answers are D-157 to D-183 in [01-decisions-log.md](01-decisions-log.md), and the implementation work they imply is listed in [dev/answers-followups.md](dev/answers-followups.md). Paths are relative to the repo root.

Remove an entry when it is decided, and record the decision in [01-decisions-log.md](01-decisions-log.md).

## Owner decisions

For each of these, the recommended answer was adopted as the default and is being implemented. The owner confirms it or overrides it.

1. **Age-restricted series are shown by default to confirmed adults.** (OQ-32, D-171) Accounts with `age_confirmed_at` now get age-restricted series included by default (`AgeFilter::Include`), with "hidden" and "only" as filter options; unconfirmed accounts never see them (404 everywhere, unchanged). A user setting `library.show_age_restricted` (default on) and the platform setting `library.age_restricted_default_visible` (default true) let a member or the operator flip the default without code. Recommendation: confirm. D-032 remembers the one-time answer permanently, so asking adults to opt in on every list contradicts it, and Discord shows age-gated channels once the gate is passed. Override by setting the platform default to `false`, which restores the opt-in behaviour. — [service.rs](../crates/sr-library/src/service.rs)

## Human actions (not decisions)

5. **pnpm is not installed on the agent machines.** (OQ-11) Install it with `corepack enable && corepack prepare pnpm@10.12.1 --activate`. Until then agents use `npx pnpm@10.12.1` with `npm_config_manage_package_manager_versions=false` and never npm for lockfile changes. — [pnpm-workspace.yaml](../pnpm-workspace.yaml)
6. **Markdown corpus review against Discord.** (OQ-15) Someone with a Discord account reviews a stratified sample of 100 corpus cases after the parser quirk decisions (Q31, Q89, Q90) are in; mismatches become hand-written expectations. — [tests/corpus](../crates/sr-markdown/tests/corpus)
7. **First green CI run.** (OQ-16) `release.yml`, `openapi.yml`, `storage.yml` and `e2e.yml` have never run on GitHub; pushing is the owner's call. FND-CI-004 stays unticked until `e2e.yml` passes. The Postgres-backed `#[ignore]` tests get their CI step in the answers follow-ups. — [.github/workflows](../.github/workflows)

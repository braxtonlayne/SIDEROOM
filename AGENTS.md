# AGENTS.md: Handoff guide for AI coding agents (and humans)

This file is the entry point for any agent assigned work on SIDE ROOM. Read it fully before touching code.

## 1. Orientation in five minutes

1. **What we're building:** [docs/00-vision-and-scope.md](docs/00-vision-and-scope.md)
2. **Binding decisions:** [docs/01-decisions-log.md](docs/01-decisions-log.md). If anything elsewhere contradicts it, the decisions log wins. Report the contradiction in your PR.
3. **How the system fits together:** [docs/02-architecture.md](docs/02-architecture.md)
4. **Which language/library to use:** [docs/03-tech-stack-and-rust-policy.md](docs/03-tech-stack-and-rust-policy.md)
5. **Your section's spec:** one of the files in [docs/specs/](docs/specs/)
6. **The atomic work list:** [docs/inventory/](docs/inventory/README.md). Each item has a stable ID (e.g. `LNG-MSG-014`).

## 2. What to read for your task

| If your task touches… | Read |
|---|---|
| Accounts, login, sessions, invites | `06-identity-accounts-invites.md`, `specs/entry-landing-and-login.md` |
| Permissions, roles, staff powers | `07-permissions-and-staff.md`, `specs/admin-staff-server.md` |
| Chat (messages, channels, servers, DMs) | `specs/lounge-chat.md`, `04-data-model.md`, `05-api-and-gateway.md` |
| Voice, video, screen share, E2EE | `specs/lounge-voice-video.md` |
| Manga/comics/ebooks | `specs/library-reader.md`, `specs/submissions-and-review.md`, `specs/ingest-naming-conventions.md` |
| Movies/shows/anime | `specs/cinema-media.md`, `specs/submissions-and-review.md`, `specs/ingest-naming-conventions.md` |
| Live TV guide | `specs/cinema-live-tv.md` |
| Watch parties | `specs/watch-parties.md` |
| Donations, perks, invite purchases | `08-donations-perks-and-cost-model.md` |
| Data retention, deletion, privacy | `09-retention-and-privacy.md` |
| Styling, themes, customization | `10-design-system-and-theming.md` |
| Bots, apps, extensions, activities | `11-extensibility-and-developer-platform.md` |
| Servers, deployment, backups | `12-deployment-and-operations.md` |

## 3. Ground rules

1. **Rust first.** All backend logic is Rust. Do not add a Node/Python/Go service. The only allowed non-Rust runtime dependencies are listed in `03-tech-stack-and-rust-policy.md` §4. Adding a new one requires an ADR in `docs/adr/`.
2. **Frontend is TypeScript + React.** Use the shared UI kit in `packages/ui`. Use Rust/WASM (`crates/*-wasm`) for parsing, crypto, validation, and heavy compute. Logic shared with the backend must come from the same Rust crate, never a reimplementation.
3. **Discord parity is the default.** If a spec is silent on a chat behavior, match current Discord behavior and note it in the PR. Exceptions are in the decisions log.
4. **Everything is a feature module.** New functionality registers through the feature registry (`11-extensibility…` §2) so it can be toggled and configured from a settings menu without new UI plumbing.
5. **No secrets in code, no telemetry to third parties.** The platform never phones home.
6. **The Board does not exist.** The landing mockups show a fourth section called "The Board." It was cut. Do not build it, link to it, or reserve routes for it.
7. **IDs and API shapes match Discord** wherever practical (snowflakes, permission bits, gateway opcodes, JSON field names). This keeps bots easy to port. See `05-api-and-gateway.md`.

## 4. Workflow

- **Claiming work:** pick inventory items (or a GitHub issue that references them). Put the item IDs in your branch name and PR title, e.g. `feat/LNG-MSG-014-message-edit`.
- **Branches:** `feat/…`, `fix/…`, `docs/…`, `chore/…`. One logical change per PR.
- **PRs must:**
  - pass CI (`cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `pnpm lint`, `pnpm typecheck`, `pnpm test`, and Playwright smoke tests once they exist);
  - include tests for new behavior (unit tests in Rust, component tests in Vitest, an e2e test for user-visible flows);
  - update docs when behavior or an API changes (the API reference is generated from code; prose specs are edited by hand);
  - tick the inventory checkboxes they complete;
  - fill in the PR template.
- **Migrations** are forward-only SQL files in `crates/sr-db/migrations/`. Never edit a merged migration.
- **The API contract** is generated from Rust (`utoipa` → OpenAPI → TypeScript client). Never hand-edit `packages/api-client/src/generated`.
- **Review:** the repository owner reviews and merges. Agents never merge their own PRs.
- **Integrator's checklist:** when a wave is merged, add its section to the top of `CHANGELOG.md` (user-facing lines written from the wave's handoffs). The first `##` heading is the version the web build embeds, so a new heading shows members the one-time "What's new" badge on the help button.
- **Wave 25 integration checklist:** bring the exit gate status table in `docs/13-roadmap-and-phases.md` to the truth. A row becomes Proven only with a link to a CI run or a handoff that records it; link the `playwright` job run of the two `@gate` specs (`developer-gate`, `moderation-gate`) from the phase 6 and 7 rows once it is green. `node scripts/check-ops-files.mjs` checks the shape of the table.

## 5. Definition of done

- The acceptance criteria in the relevant spec section are met and tested.
- The feature works in the latest Chrome, Firefox, and Safari, and as an installed PWA on Android and iOS.
- The feature respects permissions server-side. Client-side hiding is never the security boundary.
- Errors are typed (`thiserror` in libraries, `anyhow` only in binaries) and user-facing errors have copy.
- New settings appear in the correct settings menu through the feature registry.
- No `unwrap()`/`expect()` on fallible runtime paths outside tests and startup.

## 6. When you're unsure

Write the question into the PR description under **Open questions**, make the most Discord-like reasonable choice, and keep going. Don't block.

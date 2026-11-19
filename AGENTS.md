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
| Donations, perks, invite purchases | `08-donations-perks-and-cost-model.md`, `specs/donations.md` (as built) |
| Data retention, deletion, privacy | `09-retention-and-privacy.md` |
| Styling, themes, customization | `10-design-system-and-theming.md` |
| Bots, apps, extensions, activities | `11-extensibility-and-developer-platform.md`, `specs/developer-platform.md` (as built) |
| Reports, cases, takedowns, data requests | `specs/moderation-compliance.md`, `09-retention-and-privacy.md` |
| Servers, deployment, backups | `12-deployment-and-operations.md` |
| Security review, a new attack surface, authentication or storage changes | `15-threat-model.md` (one row per threat; add or update a row for a new surface) |
| Storage mounts and bring-your-own-storage (BYOS), shelves, takedown of stored files | `inventory/21-feature-phase.md` (`BYOS-*`), `09-retention-and-privacy.md`, `../deploy/GARAGE.md`, `dev/storage-live-test.md` |
| The Arcade, Activities, guest play, the emulator | `11-extensibility-and-developer-platform.md` §3, `specs/developer-platform.md`, `inventory/21-feature-phase.md` (`ACT-*`, `ARC-*`), `../deploy/EMULATOR.md` |
| The Discord bridge | `05-api-and-gateway.md` (the bridge routes), `inventory/21-feature-phase.md` (`BRG-*`), `02-architecture.md` (`sr-bridge`) |
| What a signed-out visitor, or an account that is not staff, may be served (entrance isolation, staff-only app code) | `dev/entrance-isolation.md` (D-385, D-651 to D-653) |
| The beta bug reporter | `inventory/21-feature-phase.md` (`SEC-AUD-165` to `SEC-AUD-168`), `../deploy/OPERATOR.md`, decisions D-647 to D-650 |

## 3. Ground rules

1. **Rust first.** All backend logic is Rust. Do not add a Node/Python/Go service. The only allowed non-Rust runtime dependencies are listed in `03-tech-stack-and-rust-policy.md` §4. Adding a new one requires an ADR in `docs/adr/`.
2. **Frontend is TypeScript + React.** Use the shared UI kit in `packages/ui`. Use Rust/WASM (`crates/*-wasm`) for parsing, crypto, validation, and heavy compute. Logic shared with the backend must come from the same Rust crate, never a reimplementation.
3. **Discord parity is the default.** If a spec is silent on a chat behavior, match current Discord behavior and note it in the PR. Exceptions are in the decisions log.
4. **Everything is a feature module.** New functionality registers through the feature registry (`11-extensibility…` §2) so it can be toggled and configured from a settings menu without new UI plumbing.
5. **No secrets in code, no telemetry to third parties.** The platform never phones home. **No type that holds a secret derives `Debug` (D-1033):** give it a hand-written `Debug` that prints `<redacted>` (or hold the value in the `Secret` type) and add a `{:?}` test that proves the value does not appear.
6. **The Board does not exist.** The landing mockups show a fourth section called "The Board." It was cut. Do not build it, link to it, or reserve routes for it.
7. **IDs and API shapes match Discord** wherever practical (snowflakes, permission bits, gateway opcodes, JSON field names). This keeps bots easy to port. See `05-api-and-gateway.md`.

## 4. Workflow

- **Claiming work:** pick inventory items (or a GitHub issue that references them). Put the item IDs in your branch name and PR title, e.g. `feat/LNG-MSG-014-message-edit`.
- **Branches:** `feat/…`, `fix/…`, `docs/…`, `chore/…`. One logical change per PR.
- **PRs must:**
  - pass CI (`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace` (and `cargo deny check`), `pnpm lint`, `pnpm typecheck`, `pnpm test`, the repo checks (`.github/workflows/repo-checks.yml`: PR title, doc links, ops files, secret scan, script tests), the web build checks (bundle budget, entrance isolation, staff delivery) and the Playwright suites in `e2e/`; `just lint` and `just test` run the local equivalents);
  - include tests for new behavior (unit tests in Rust, component tests in Vitest, an e2e test for user-visible flows);
  - update docs when behavior or an API changes (the API reference is generated from code; prose specs are edited by hand);
  - tick the inventory checkboxes they complete;
  - pass the **visual check** when they add or change anything a person sees (below);
  - fill in the PR template.
- **Visual check (owner rule, 2026-11-02):** a change that adds or alters a page, dialog, panel or other visible UI is not done until it has been looked at. It is part of the work, not a separate round.
  1. **New routes go into the sweep.** Every new route (and every new public surface) gets an entry in `e2e/tests/page-sweep.spec.ts`, with the data it needs to look real, so the sweep always covers the whole app.
  2. **The builder shoots its routes.** Run the sweep for the routes it touched (`SR_E2E_SWEEP=1 SR_SWEEP_ONLY='<route regex>'`, see `docs/dev/handoff/fix-visual-sweep.md` "How to run the sweep"), at desktop 1280x800 and phone 375x812, in Lounge Light and the dark theme. It reads the PNGs itself and fixes what looks wrong: overflow, overlap, clipped or off-screen controls, raw i18n keys, broken empty/loading/error states, unreadable contrast, theme not applied. The sweep's automatic checks (errors, failed requests, blank page, phone overflow, raw keys) must be clean for those routes, or each remaining flag explained.
  3. **The builder clicks through its flows** the way a person would (create, edit, save, delete, navigate back) and fixes what breaks.
  4. **The handoff lists the routes it shot** and where the screenshots are, with before/after for anything it fixed.
  5. **The integrator re-shoots every route the wave touched on the merged tree** (both viewports, both themes), looks at them, and fixes or sends back anything wrong before the PR is opened. A PR that changes visible UI without this does not merge.
  6. **Section themes (D-797).** The Library, Cinema and Arcade keep their own theme, set under Settings > Appearance > Per-section themes, not the main theme. Shoot their pages in the section's shipped theme and once with a light section theme; changing the main theme does not change them. Both the shipped theme and `lounge-light` must draw the Library and the Cinema whole (wave 78, D-802, D-803). A page of a room is measured for sideways scroll at 375 and 1280 (D-817); the Arcade is measured as the Library and the Cinema are (D-822); the Study is a Lounge-family room and follows the member's main theme, not a section theme (D-1010), and is measured with the Lounge, and so is every other signed-in page (D-848).
  7. **Specs the sweep or a flow uses must load in CI without local files:** call `test.skip(...)` first, read `e2e/.state/*` only inside tests, and never commit scratch probes.
  8. **Touch targets (D-917):** a spec that hit-tests a touch target imports `openCollapsed` and `hitTestAll` from `e2e/support/hit-test.ts`; a "not reached" is a probe error, not a fault. A phone pass on a skin is measured at 375 and 700 (D-934). A knob or other control worked through a wrapper is hit-tested with the helper's `wrapper` option (D-939). A shot that shows crowding is compared with the base at the same width before it is judged. An open menu is hit-tested with `hitTestAll(page, menu, { also: MENU_ITEM_SELECTOR })` (D-941). A spec that creates named data in the shared sweep world gives it names no other spec uses, and a cached `ids.json` carries the world's guild id (D-942). A shot of an open menu is taken with the pointer resting on a plain item and no submenu open, and the spec asserts it (D-943). A spec with two clients waits on the client it reads (D-944), and a skin's panel is shot at 375 and at 700 (D-946). A table wrapper that scrolls sideways is a named, focusable region (D-948), and a stack that seeds invoices sets a public rate source. An Admin table goes in `TableRegion` (D-949), never in a bare `adm-table-wrap`. A short fixed list uses the kit's `NativeSelect` (D-950). A spec that reads pixels at a box's edge uses `e2e/support/edge-strips.ts` (D-957) and asserts on the rows it made itself. A confirm-first test runs at the data size where the fault would show (D-961). Test data only some specs need comes from a support helper with its own names and a `remove()`, never the shared world seed: a spec that needs an app with a choice command calls `seedChoiceCommand` of `e2e/support/p102-app.ts` and removes it in a `finally` (D-968). An area sheet does not style a kit select by element name (`.x select` beats the kit's padding and height): write `.x > select` or move the select (D-967). The heavy lock is a queue: a ready builder takes a free lock at once and never waits for a clock time (D-970). A select that sat as wide as its content takes the kit's `fit="content"` (D-971). No area sheet gives a `select` a box by element name; a shared sheet rule is changed only with its dependants named and measured (D-972). At 375 with touch a hit test uses the default reach, at pointer widths `reach: 'box'` (D-983). A shots spec proves its own wave (D-984). A select that fills its field keeps the default `fit="fill"`, no sheet styles the kit's wrapper, and a hit-test fail with a taker is a fault at either width. The web typecheck runs only through `cli-agents/tc.sh`. A mechanical group of builders is a pod: code builders without a stack and one shots builder who owns the spec and opens every shot. A builder stops every process it started by Windows PID, then lists what still runs from its worktree and what still listens on its ports, and puts both lists in its handoff before it replies; nothing is stopped by image name, and the integrator runs `cli-agents/strays.sh` over every worktree before removing one (D-989). A cure names the rows it may move: a task or FAULTS entry that asks for a layout fix lists the elements allowed to change box and by how much, and anything else that moves is a fault of the cure (D-990). On a phone a control under 44 px is cured in this order: a tap area that keeps its drawn size where the 44 px covers no neighbour's centre line, else the control and its row grow, and wrapped rows of small buttons get a row gap; a link, mention or name inside running text is exempt, and a `::after` tap area on a bordered control is inset by the reach plus the border width (D-1022). An area's own field keeps its box and takes the kit field's border mix at rest (65%), `--ui-text-faint` on hover, a 2 px `--ui-focus` ring, a faint placeholder and 44 px on a phone, and no rule removes a field's outline without a ring on its wrapper (D-1023, D-1024). A test point outside the viewport is not tested (the row records `edge`); a control passes when every point inside the viewport passes, and a control under 44 px at the screen's edge is also tested at the far side of a 44 px span measured from the edge (the `span` point), so a link at the top of a phone must reach 44 px on screen and not hide half its target off screen (D-1025). A tap area's inset counts the control's border, and an area a later row paints over does not count: the row gets a margin (D-1022). A test against a real bucket is run by the commander and the owner only: no agent handles, reads or logs a credential, and no cell has an object store (D-1029, D-1030).
  9. **Shots go through the job runner (D-1004):** shots are taken in built mode through `cli-agents/job.sh`; you never start a server, Vite or Playwright yourself. `shoot base` serves the wave's base bundle and `shoot after` serves your own build; a tree edited after its build is refused with `stale build` (rc 3), so build again. A spec reads only `SR_E2E_BASE_URL`, never imports a `/src/...` module, uses the seeded sweep world, and compares in the `after` phase only when `figures-base.json` is there. A web typecheck and a web build are jobs too. A web typecheck costs about 3.3 GB and a cell about 1 GB, and a job waits longer than it runs, so submit fewer, fuller jobs and read a failed job's log before submitting again. The runner's scripts are copied into `scripts/dev/runner/` (commit hash in `SOURCE`) and documented in `docs/dev/job-runner.md`; the running copy stays in `cli-agents/`, is changed only in a clone and between waves, and the repository's copy is refreshed in the next wave's docs commit (D-1027). A builder runs no vitest, eslint, web `tsc`, i18n audit or doc-link check, and only one prettier run of its own files at a time (e2e `tsc` for a spec owner): the commander runs them once on the merged tree, and a launch of more than eight builders is staggered (D-1028).
  10. **FINDERS (D-1020):** before it commits an app change, a builder greps `e2e/` and the unit tests for whatever finds or drives the controls it changes and lists them under FINDERS; a CI spec or test that drives a changed control is fixed in the same branch (wave 106 itself missed one: `entranceDangerText` banned any `color-mix`).
- **Migrations** are forward-only SQL files in `crates/sr-db/migrations/`. Never edit a merged migration.
- **The API contract** is generated from Rust (`utoipa` → OpenAPI → TypeScript client). Never hand-edit `packages/api-client/src/generated`.
- **Review:** the repository owner reviews and merges. Agents never merge their own PRs.
- **Integrator's checklist:** when a wave is merged, add its section to the top of `CHANGELOG.md` (user-facing lines written from the wave's handoffs). The first `##` heading is the version the web build embeds, so a new heading shows members the one-time "What's new" badge on the help button.
- **Wave 25 integration checklist:** bring the exit gate status table in `docs/13-roadmap-and-phases.md` to the truth. A row becomes Proven only with a link to a CI run or a handoff that records it; link the `playwright` job run of the two `@gate` specs (`developer-gate`, `moderation-gate`) from the phase 6 and 7 rows once it is green. `node scripts/check-ops-files.mjs` checks the shape of the table.
- **Before integrating (from wave 28):** run `crates/sr-server/tests/immutable_rules.rs` (`SR_TEST_DATABASE_URL=... cargo test -p sr-server --test immutable_rules -- --include-ignored --test-threads=1`). It proves that no flag, setting or feature can weaken end-to-end encryption of calls, per-route authorization or the audit and access logs. An agent that adds a feature or setting never names it with `e2ee`, `encrypt`, `audit`, `access_log`, `authz`, `authorization` or `permission`.
- **Feature phase (waves 27 to 40) integration checklist:** tick `docs/inventory/21-feature-phase.md` only where the proof ran; run the full authorization matrix with the storage actors (`storage_scopes_hold_over_http` and `every_route_enforces_its_authz_row`) and, from wave 28 on, `crates/sr-server/tests/immutable_rules.rs`; check job ids, setting keys and access-log kinds for duplicates; regenerate OpenAPI, the api-client and the MSW handlers (`mocks:check` green). The live Garage cases of `storage.yml` run only on CI.
- **Wave 39 integration gate (OPS-IMM-003):** `immutable_rules.rs` now also walks the whole feature registry (a feature a wave registers is covered with no edit), runs the refusals in a third state (every guild module off, then every feature killed), and proves that the name test catches a test-only `voice.e2ee_test`. Run it before integrating (`cargo test -p sr-server --test immutable_rules`, then the ignored run above). The weekly `security-nightly.yml` (also on dispatch) runs the whole authorization matrix, read and write phases, in the normal state and with the flags killed and fails if the two disagree on any refusal; dispatch it on the integration branch before the wave 39 PR merges.

## 5. Definition of done

- The acceptance criteria in the relevant spec section are met and tested.
- The feature works in the latest Chrome, Firefox, and Safari, and as an installed PWA on Android and iOS.
- Every page it adds or changes is in the page sweep and has passed the visual check (section 4): shot at desktop and phone width in light and dark, looked at, and fixed.
- The feature respects permissions server-side. Client-side hiding is never the security boundary.
- No hard-coded user-visible text in the web app: the text is a message key, `t('...')`. `node scripts/i18n-audit.mjs --max 0` must pass (it runs in the Repo checks workflow and prints the files that fail).
- Errors are typed (`thiserror` in libraries, `anyhow` only in binaries) and user-facing errors have copy.
- New settings appear in the correct settings menu through the feature registry.
- No `unwrap()`/`expect()` on fallible runtime paths outside tests and startup.

## 6. When you're unsure

Write the question into the PR description under **Open questions**, make the most Discord-like reasonable choice, and keep going. Don't block.

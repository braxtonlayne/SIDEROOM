# Open questions

Questions raised by implementation work that need an owner decision. Each links to the code it affects. Paths are relative to the repo root and resolve once the originating branch is merged. Remove an entry when it is decided (and record the decision in [01-decisions-log.md](01-decisions-log.md)).

## Auth (`feat/AUTH-primitives`)

1. **Recovery code strength.** Codes are 60 bits, stored as argon2id hashes; checking up to 10 hashes makes each attempt slow, so rate limiting matters. Longer? — [recovery.rs](../crates/sr-auth/src/recovery.rs)
2. **TOTP secret encryption key source.** Secrets must be encrypted at rest; where does the key come from (config, KMS)? Not wired in this crate. — [totp.rs](../crates/sr-auth/src/totp.rs), [repo.rs](../crates/sr-auth/src/repo.rs)
3. **Hash invite codes at rest?** Kept plain/canonical because doc 06 treats the code as a lookup key. — [invite.rs](../crates/sr-auth/src/invite.rs)
4. **Argon2 and session defaults.** Chosen by the agent, not from docs: argon2id 64 MiB / 3 passes / 1 lane; sessions 30 days idle, 180 days absolute; reset token 30 min (doc 06 §4), verify token 24 h. — [password.rs](../crates/sr-auth/src/password.rs), [session.rs](../crates/sr-auth/src/session.rs), [email_token.rs](../crates/sr-auth/src/email_token.rs)
5. **Password strength (ENT-AUTH-004, zxcvbn) and CSRF (ENT-AUTH-006)** are not implemented; should they live in `sr-auth`? — [crates/sr-auth](../crates/sr-auth)

## Core (`feat/FND-CORE`)

6. **Discord's extra permission rules** (timed-out members lose most permissions; no VIEW_CHANNEL means nothing) are not implemented. Add them, and where? — [permissions.rs](../crates/sr-core/src/permissions.rs)
7. **Browser clock.** `SystemClock` panics on `wasm32-unknown-unknown`; the wasm crate needs a `Date.now()` clock. — [time.rs](../crates/sr-core/src/time.rs)
8. **Permission fixtures** are hand-derived, not captured from real Discord. Capture real ones? — [crates/sr-core/tests](../crates/sr-core/tests)
9. **Error-code list** is a subset of Discord's; extend as endpoints need. — [error.rs](../crates/sr-core/src/error.rs)
10. **Config schema/defaults** are a guess; review against doc 12. — [config.rs](../crates/sr-core/src/config.rs)

## Database (`feat/FND-DB`)

11. **Migrations never run against Postgres.** Needs a run with `SR_TEST_DATABASE_URL` set (`cargo test -p sr-db -- --ignored`). — [migrations](../crates/sr-db/migrations)
12. **Additions beyond doc 04:** `login_events` (180-day IP history, doc 09), `accounts.age_confirmed` (doc 06 §7), job queue as migration 0004. Confirm. — [0001_identity.sql](../crates/sr-db/migrations/0001_identity.sql), [0004_jobs.sql](../crates/sr-db/migrations/0004_jobs.sql)
13. **sqlx TLS features not enabled**; connections are plaintext until added. — [pool.rs](../crates/sr-db/src/pool.rs)
14. **FND-DB-007** (monthly `messages` partition manager) not started.

## Events and features (`feat/FND-EVT-FEAT`)

16. **`Feature::routes()`, `FeatureCtx`, `Migration`** are placeholders until `AppState` exists. — [feature.rs](../crates/sr-features/src/feature.rs)
17. **Merge conflict:** root `[workspace.dependencies]` was added on both this branch and `feat/FND-CORE`. — [Cargo.toml](../Cargo.toml)

## Web (`feat/FND-REPO-004-web`)

18. **FND-API-007 is ticked but is a stub** (no HELLO/IDENTIFY/heartbeat/RESUME). Untick? — [gateway.ts](../apps/web/src/api/gateway.ts)
19. **Pre-commit hook not installed** (lint-staged configured only; FND-REPO-007). — [package.json](../package.json)
20. **pnpm not installed locally**; the agent used `npx pnpm@10.12.1` with `npm_config_manage_package_manager_versions=false`. — [pnpm-workspace.yaml](../pnpm-workspace.yaml)

## Ops (`chore/FND-OPS`)

21. **Pinned versions are guesses** (Garage v1.0.1, Mailpit v1.21) and the Garage bootstrap CLI flags are unverified. — [dev.yml](../deploy/compose/dev.yml), [bootstrap.sh](../deploy/compose/garage/bootstrap.sh)
22. **Unvalidated:** `deny.toml` and `garage.toml` were never parsed; nothing was run in docker/just/cargo. — [deny.toml](../deny.toml)
23. **Workflows depend on missing pieces** (Dockerfiles under `deploy/docker/`, `@sideroom/wasm` and `@sideroom/api-client`, `sr-cli seed`, `sr-server openapi`, `*-wasm` crates), so release and OpenAPI drift jobs fail for now. — [release.yml](../.github/workflows/release.yml), [openapi.yml](../.github/workflows/openapi.yml)

## Storage (`feat/FND-STOR`)

24. **Client-driven/resumable multipart uploads are not possible** with `object_store`: `multipart_create` is server-side only. Presigned part URLs would need the raw S3 API or another crate. Needed? — [storage.rs](../crates/sr-storage/src/storage.rs)
25. **"30 MB" read as 30 MiB** (and 500 MiB, 64 MiB multipart threshold). Confirm binary vs decimal. — [upload/](../crates/sr-storage/src/upload)
26. **EXIF stripping drops the orientation tag.** Preserve orientation (rotate pixels instead)? — [upload/](../crates/sr-storage/src/upload)
27. **Presigned URLs against real S3/Garage are untested**; only the builder and the in-memory fake are covered. — [storage.rs](../crates/sr-storage/src/storage.rs)
28. **FND-STOR-004 left unticked**: the upload-slot endpoint belongs in `sr-api`. FND-STOR-003 (image resizer) untouched. — [crates/sr-storage](../crates/sr-storage)

## Markdown (`feat/LNG-markdown`)

29. **Mention cap of 100 is an assumption**, not from the docs. Confirm the limit. — [crates/sr-markdown](../crates/sr-markdown/src)
30. **WASM not built.** No `wasm` feature or wasm-bindgen yet; the `wasm32-unknown-unknown` target isn't installed here. The crate is pure `std` + serde. (LNG-MD-005 also needs the 500-case shared corpus; ~150 exist.) — [lib.rs](../crates/sr-markdown/src/lib.rs)
31. **Shortcodes and jumbo-emoji detection (LNG-MD-003) not implemented.** — [crates/sr-markdown](../crates/sr-markdown/src)
32. **Parsing quirks to confirm against Discord:** a fenced block is split out wherever it appears (mid-line or inside a quote); `>>>` quotes up to the next code fence; `* text` at line start is a list item, not italic. — [crates/sr-markdown](../crates/sr-markdown/src)

## UI kit (`feat/DS-tokens-prims`)

33. **Token types are hand-written**, not generated from the `sr-theme` schema (DS-TOK-001), which doesn't exist yet. — [packages/ui/src/theme](../packages/ui/src/theme)
34. **Font files missing.** Font config is done (DS-TOK-005) but WOFF2 binaries and licence files must be added by the app. — [fonts.ts](../packages/ui/src/theme/fonts.ts)
35. **`sideroom` token values were not adjusted after viewing the mockups**; they follow docs/10 only. Compare with [docs/assets/mockups](assets/mockups/README.md). — [packages/ui/src/theme](../packages/ui/src/theme)
36. **Tooltip/Popover don't flip at viewport edges** (plain CSS placement). Add a positioning library or custom logic? — [packages/ui/src](../packages/ui/src)
37. **Saturation setting uses a CSS `filter` on the provider root**, which makes `position: fixed` overlays position relative to that root while active. — [packages/ui/src/theme](../packages/ui/src/theme)
38. **Partial primitives left unticked:** Slider, Segmented, ContextMenu, Dropdown, Drawer, Accordion, AvatarStack, Pill/Tag, ProgressBar, EmptyState, error page; DS-PRIM-004, 012–016 (Select, ScrollArea, icons, Ladle stories) not started. — [01-foundation.md](inventory/01-foundation.md)
39. **Verified with npm/npx and no lockfile** (pnpm not installed); the package's lockfile and workspace linking are untested. — [packages/ui/package.json](../packages/ui/package.json)

## UI kit: misc primitives and SR chrome (`feat/DS-prims-misc`)

40. **DS-PRIM-009 left unticked:** Avatar has no "animated on hover" behaviour (needs a static-frame/animated source pair from the media pipeline). The decoration slot and AvatarStack are done. — [Avatar.tsx](../packages/ui/src/primitives/Avatar/Avatar.tsx)
41. **DS-SR-008 left unticked:** the door mark is an SVG component plus `packages/ui/assets/door-mark.svg`, but the favicon `.ico` and PWA PNG icon sizes (192/512, maskable, apple-touch) are not generated. Who produces the final artwork? — [packages/ui/assets](../packages/ui/assets)
42. **DS-PRIM-016 left unticked:** Ladle is configured and builds with stories for this batch only; the other primitives (Button, inputs, Modal, Tabs, menus, Select, etc.) need stories and axe coverage, and CI doesn't run `ladle build`. axe runs in jsdom with color-contrast disabled (no layout). — [packages/ui/.ladle](../packages/ui/.ladle)
43. **DatePicker/TimePicker wrap native inputs** (`type=date/time/datetime-local`), so the calendar popup styling and first-day-of-week follow the browser. Build a custom calendar grid for a consistent look? — [DateTime.tsx](../packages/ui/src/primitives/DateTime/DateTime.tsx)
44. **DropdownChip uses a native `<select>`**, so its popup can't be themed. Swap to the Select/Popover primitives once they land? — [FilterChips.tsx](../packages/ui/src/sideroom/FilterChips.tsx)
45. **Icon set is a first pass:** about 60 round-stroke icons on the 24 grid, scaled for 20/16 by stroke weight only (no hand-tuned 16px variants). Needs a design review against the mockups; the `settings` glyph is a sliders icon, not a gear. — [paths.ts](../packages/ui/src/icons/paths.ts)
46. **`@testing-library/dom` and `axe-core`, `@ladle/react` added to `packages/ui` devDependencies**; `pnpm-lock.yaml` was not regenerated (pnpm not installed). Run `pnpm install` before merging. — [package.json](../packages/ui/package.json)
47. **Drawer duplicates Modal's focus-trap code** and does not join Modal's stack, so a Modal opened from a Drawer (or vice versa) can have both handle Escape. Extract a shared overlay hook? — [Drawer.tsx](../packages/ui/src/primitives/Drawer/Drawer.tsx)

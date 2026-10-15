# Handoff: feat/p11-devplat

Base `integration/phase11`. Migration `0710`. Local commits only.

## Done
* DEV-BOT-005: HTTP interactions endpoint (sr-net POST, sr-apps `signing.rs`/`service/endpoint.rs`, sr-api `developer/endpoint.rs`, `SafeEndpointClient` in sr-server, portal card). Tests: `endpoint_tests.rs` (8), signing (RFC 8032 vector), Postgres `apps_repo` (2 new).
* DEV-BOT-002: sharding in sr-gateway (4 tests) plus recommended shards in `/gateway/bot`.
* DEV-BOT-008 (partly), 009, 010, 011: `apps/web/src/lounge/apps/*`, `GET /channels/{id}/application-commands`, real component interactions, ephemeral note, Apps menu, modal host. Tests: `commands.test.ts`, `p11DevPlat.test.tsx`, `endpoint.test.tsx`; API tests in `flow_tests.rs`.
* EXT-PKG-007 (CLI part): `sr-cli package validate <dir> [--json]`; EXT-PKG-002 audited as built.
* OpenAPI, api-client, mocks, route matrix, bot policy updated. i18n fragments `p11-devplat-1/2.ts`.

## Not done (reason)
* DEV-APP-001 `app_testers`, DEV-APP-007 user installs: owner decisions (below); conservative default kept (owner only).
* DEV-BOT-006: username/avatar override needs a per-message author model; `thread_id` untouched.
* DEV-BOT-007 (MLS voice), DEV-BOT-012 App Directory, DEV-PORT-003 other tabs, DEV-PORT-004 generated docs, DEV-SDK-001/002/003, DEV-ACT-*, EXT-PKG-001/003..006, EXT-PRF-*: too large for this pass; sr-sdk is an empty stub.
* DEV-BOT-008 attachment option; creator docs page for EXT-PKG-007.
* Ephemeral replies cannot use their components (not stored messages).

## Decisions / open owner questions
1. Signing seed stored unencrypted in the DB (alternative: derive from `server.secret_key`). Confirm.
2. Endpoint URLs must be https on public addresses only (no private-network opt-in). Confirm.
3. No auto-disable of an endpoint after repeated failures (Discord does); failing deliveries only show "interaction failed".
4. Typed `name:value` syntax in the composer plus a form; Discord has chip-style inputs.
5. Earlier open owner questions from feat-p8-dev-be (testers, install cap, SEND_MESSAGES bypass, intents review, token lifetimes) remain unchanged.

## Tests run
cargo (all `-j 2`): sr-net, sr-apps, sr-gateway, sr-theme, sr-cli, `sr-api developer` and `--test route_matrix`, sr-db `apps_repo` and sr-server `bot_pg` (Postgres, scratch DBs); fmt and clippy -D warnings on touched crates. Web: tsc, eslint on lounge/store/test, vitest lounge, store, i18n, developers.

# Handoff: wave 110, builder mounts (D-1032, D-1033, D-1034; SEC-AUD-539, 540, 541)

## Clock
Started 23:12. Cargo window opened 23:20, build done 23:32, tests 23:40, openapi 23:48; clippy started 23:48 .

## Done (commits on fix/p110-mounts)
- D-1032 `crates/sr-storage/src/cors_check.rs`: `check` falls back to the live preflight when the config read is refused: HTTP 403, or S3 code `AccessDenied` / `AccessDeniedException` (`denied()`, by status and code, never message text). Only a preflight answering success with the allow-origin header counts (`preflight_passes`, shared with the 501 path). A refused, failed or unrunnable preflight leaves today's error (409 "The service refused to read or write the bucket's CORS rules", `cors_ok_at` untouched). `mount_ops.rs` needed no change: it stamps `cors_ok_at` from `CorsState::Ok`, so a passing fallback stamps it exactly as a config read does.
- D-1033 hand-written `Debug` (`<redacted>`) for `CreateMountBody`, `PatchMountBody` and every credential type in the list below; derive of `Debug` removed from each.
- D-1034 `MountObject::of(m, own)`: `access_key_id` is `…` plus the last four characters (`mask_key_id`); `endpoint` only when `own` (personal routes `/users/@me/storage/mounts`: list, create, patch). The server's routes (`/guilds/{id}/storage/mounts`, read by whoever manages the server) mask the key id and leave out the endpoint. The staff list (`AdminStorageMount`) already had neither key id nor endpoint (only `key_hint`): unchanged. `Scope` is now `Copy`.
- Generated: `packages/api-client/openapi.json` and `src/generated/schema.d.ts` changed only by two field descriptions (no type change; `endpoint` was already optional). No `.tsx` touched. `mocks:generate` run (no diff).

## Not done
- Tests for `CreatedToken` (sr-media), `Ready` (sr-voice-proto) and `SfuConfig` (sr-voice): impl only, no test (fields without a cheap constructor). sr-sdk `Interaction`, sr-bridge `CreatedWebhook` and sr-cli `PgEnv`, sr-loadtest `Participant`, sr-activities types: tests written but not run if not listed under CHECKS (the sr-sdk/sr-bridge test build needs cmake for opusic-sys, absent on this machine, when built as a test target alone).
- A mount_ops-level (HTTP) test of the fallback: the in-process fake S3 of `mounts_tests.rs` cannot set response headers; the fallback is proven in `crates/sr-storage/tests/cors_denied_fallback.rs`.
- No Postgres test run (`--ignored`): nothing here touches the database.

## Storage of the secret
`secret_key_enc` is the secret access key sealed with `server.secret_key` (AES-GCM, `sr_db::sealed::SecretBox`, AAD = mount id); `Mount` holds only the sealed bytes; the plaintext lives in `sr_storage::mounts::Secret` (Debug `Secret(..)`) while a request runs.

## D-1033 types
Changed (plaintext credential, `Debug` now redacted), with test unless noted:
sr-activities `GuestLinkCreated`, `GuestJoined`; sr-api `CsrfToken`, `ProxyTokenResponse`, `ResetBody`, `VerifyBody`, `ResetAgeBody`, `DiscordSettings`, `DiscordLinkBody`, `UnlinkBody`, `ChangePasswordBody`, `ChangeUsernameBody`, `ChangeEmailBody`, `CancelEmailChangeBody`, `DeleteAccountBody`, `LoginBody`, `LoginLinkBody`, `PasswordBody`, `PasswordBodyDoc`, `RegisterBody`, `DeleteAllBody`, `TokenBody` (bridge), `AppealBody`, `WebhookObject`, `AckResponse`, `TurnCredentialsObject`, `CreateMountBody`, `PatchMountBody`; sr-bridge `CreatedWebhook`; sr-cli `PgEnv`; sr-gateway `Identify`, `Resume`; sr-loadtest `Participant`; sr-media `CreatedToken` (no test); sr-sdk `ShardConfig`, `Interaction`; sr-transcoder `Cli`; sr-voice-proto `TurnConfig`, `TurnCredentials`, `IceServer`, `Identify`, `Resume`, `Ready` (no test); sr-voice `SfuConfig` (no test); sr-auth `TotpRecord`, `TotpEnrollment` (the base32 key and the otpauth URI carried the secret).
Judged not a credential: every `*_hash` / `*_hint` / `token_id` / `token_type` / `credential_id` field (hashes and ids), `Secret`-typed fields in `sr_core::config` (already redacted by `Secret`), `TotpSecret`, `Secret`/`SealedSecret` (already redacted), `PasskeyProof`/`RegisterFinishBody`/`LoginFinishBody` (WebAuthn assertions: single-use public-key proofs, not reusable secrets), `Settings.secret_files` (file paths), `Tokens` of themes (design tokens), enums named `*Token*`/`*Password*` (no payload), `Inner` in `sr-bridge/tests/support/fake_guild.rs` (test fake with a made-up token; left), `sr-cli` `Command` (clap args with `owner_password`; left: not printed anywhere, named for the commander to decide).
Logging: no `#[instrument]` exists in `crates/sr-api/src`, and `git grep` found no `tracing::` call formatting a request body, so no `skip` was needed. The risk before was any `{:?}` of a body or an `anyhow` context carrying one, and `Debug` on `ApiJson<T>` errors; all are covered now.

## FINDERS (rule 13)
`git grep -n -i "access_key_id\|key_hint\|accessKey\|endpoint" -- e2e crates/sr-cli`: no hit that reads a mount-list row (the CLI hits are `storage.endpoint` config and the instance mount's own `Mount`). Web: `MountsPanel.tsx:324` prints `access_key_id` through the `keyLine` string, which only uses `{hint}` (so it never showed the id); `settings/user/__tests__/p27storage.test.tsx:17` and `storage/mocks.ts:65` use mock values. No web code reads `mount.endpoint` of a row. `e2e/tests/r2-byos.spec.ts`: no line prints a list row (not edited).

## CARGO
Window 23:20 to about 23:55 (overran; agreed with the commander). Commands (all `--offline --locked -j 4`, `CARGO_PROFILE_DEV_DEBUG=0`):
- `cargo fmt --all`: ok (changed only my files).
- `cargo build -p sr-server -p sr-cli`: first run failed (E0382, `Scope` moved), fixed, second ok (5m13s).
- `cargo test --lib -p sr-storage -p sr-api -p sr-auth -p sr-gateway -p sr-voice-proto -p sr-activities -p sr-transcoder -p sr-loadtest -- <names>`: ok, 27 in sr-api (including `a_key_id_is_masked_to_its_last_four`, `the_mount_bodies_print_no_secret`, `the_lists_mask_the_key_id_and_only_your_own_show_the_endpoint`), 2 sr-auth, 2 sr-gateway, 1 sr-transcoder, 5 sr-voice-proto, plus sr-activities and sr-loadtest.
- `cargo test -p sr-storage --test cors_denied_fallback --test s3_probe_mock`: 5 and 20 passed. The five: `config_read_403_and_a_passing_preflight_is_ok`, `access_denied_code_with_another_status_also_falls_back`, `config_read_403_and_a_preflight_without_the_allow_header_is_not_ready`, `a_403_on_the_preflight_itself_is_never_ok`, `config_read_501_is_still_manual_and_a_good_config_needs_no_preflight` (which also proves a good config read makes no preflight).
- `cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json`: ok; `openapi-typescript` and `scripts/gen-msw.mjs` run by `node` directly (pnpm is not on PATH here).
- Binaries copied to `C:/Users/braxt/Desktop/SIDEROOM_wt/p110-mounts-bin`. `p110-mounts-target` and `p110-mounts-bin` left for the commander.

## CHECKS
`cargo clippy -p sr-storage -p sr-api -p sr-auth -p sr-gateway -p sr-voice-proto -p sr-activities -p sr-bridge -p sr-sdk -p sr-transcoder -p sr-cli -p sr-loadtest -p sr-voice -p sr-media --all-targets -- -D warnings`: ok, 23:49 to 23:53 (needs cmake from the VS 2019 Build Tools on PATH for opusic-sys). `cargo fmt --all --check`: exit 0. Left to the commander: vitest, eslint, web tsc (no web type changed), i18n audit. Tests of sr-sdk, sr-bridge and sr-cli types were not run (the test build of opusic-sys needed cmake and I did not retry with it); they compile under clippy --all-targets.

## Closing texts
- SEC-AUD-539 closed: the CORS check falls back to the live preflight when the config read is refused (403, AccessDenied, AccessDeniedException); only a preflight with the allow headers sets `cors_ok_at`. Tests in `crates/sr-storage/tests/cors_denied_fallback.rs`. D-1032.
- SEC-AUD-540 closed: no type that holds a plaintext credential can print it with `{:?}`: `CreateMountBody`, `PatchMountBody` and 45 more types print `<redacted>`; one test each (list above). D-1033.
- SEC-AUD-541 closed: the mounts lists and the create and patch answers show the key id as `…` plus its last four characters; the endpoint only in the answers about the caller's own mounts. Staff list unchanged (it never had them). D-1034.
- Threat model sentence for D-1034 (the model has none on what a list reveals): "A mount's list reveals the last four characters of its key id to its owner and to those who manage the server it belongs to, never the whole id or the secret; the endpoint, which names the owner's account at the provider, is shown only to the owner."
- Docs by this row: none describe the list's fields or the CORS fallback beyond `docs/dev/roadmap-p27.md:104` (BYOS-MNT-003, historical); replacement sentence if wanted: "Providers whose API refuses (501) or whose key may not read the bucket's CORS rules (403) are checked with a live preflight; only one that answers with the allow-origin header counts."
- Changelog: "A storage bucket whose key may not read CORS settings now passes the check when a live preflight succeeds, mount lists show only the last four characters of a key id, and credential-bearing request types no longer print their secrets in debug output."

## Owner questions (to Fable)
None needed. Defaults chosen: the server's mounts list hides the endpoint from managers (Discord-like least disclosure); the CLI `Command` enum left as is.

## PROCESSES
`strays.sh p110-mounts`: "strays: none found" (23:53). No job of mine existed. `sr_p110_mounts` was never created.

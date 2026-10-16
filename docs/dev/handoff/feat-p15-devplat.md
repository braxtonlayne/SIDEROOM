# Handoff: feat/p15-devplat (developer platform: packages, profile pages, bots in voice)

Branch `feat/p15-devplat`, based on `integration/phase15` (= main). Migrations `0970` and `0971`. Local commits only. Spec: [developer-platform.md](../../specs/developer-platform.md) sections 3.11 to 3.13. Inventory: [17-developer-platform.md](../../inventory/17-developer-platform.md). D-063, D-064, D-185, D-186, D-203 to D-206, D-220, D-234 to D-237, D-246 to D-250 are untouched.

## Audit result

| ID | Found | Result |
|---|---|---|
| EXT-PKG-001, 003, 004, 005, 007 | `sr-theme` validator and `sr-cli package validate` for themes only; nothing else | built, ticked |
| EXT-PKG-006 | nothing | built except adding a pack's files to a server and a server-wide theme; box stays open |
| EXT-PRF-001 to 006 | `accounts.profile_doc` column existed and was unused | built, ticked (two deviations, below) |
| DEV-PORT-004 | hand-written hub; a Scalar page at `/developers/docs/api` already read the OpenAPI document | API and gateway reference pages generated, ticked |
| DEV-BOT-007 | no voice API in either SDK | the MLS (key agreement and frame) half built in `sr-sdk`; media transport not built; box stays open |
| DEV-APP-007 user installs | owner decision | untouched, conservative default (server installs only) kept |
| DEV-SDK-003 | needs downloads | untouched, the owner's CI action |

## Done

- **`crates/sr-packages`** (new). `zipfiles` (safe zip reader: 8 MiB, 400 entries, path rules, counted while inflating), `validate` (one validator for all seven kinds; themes through `sr-theme`; recipe and template checks come in through a `Checks` struct so the CLI can run without a server), `profile_doc` (schema and validator), `service` (submit, review, browse, enablements, profile documents, guestbook). Migration `0970_packages.sql` (`packages`, `package_versions`, `package_enablements`) and `0971_profile_guestbook.sql`.
- **API (`sr_api::routes::packages`)**: creator (`POST /packages` with the zip as body, `GET /packages/mine`, withdraw), browse (`GET /packages`, `/packages/{id}`, pack files), person (`/users/@me/packages`), server (`/guilds/{id}/packages`, `apply` for recipes, `/packages/{id}/create-guild` for server templates), profile (`/users/@me/profile-doc`, layout adoption, `/profile-pages/{username}`, `/users/{id}/profile-page`, guestbook), staff (`/admin/developers/packages*`, access log entries). Bots are refused on all of them (`auth::policy`). Route matrix rows reviewed with the G-19 vocabulary (no `?`); OpenAPI, api-client schema and MSW handlers regenerated; `openapi_names` `KNOWN_CLASHES` is still empty (new schema names are prefixed with `Package` or `Profile`).
- **Recipe applier.** Every entry is validated at upload (known module, settings against its own schema, no channel or role ids because a recipe is shared) and again when applied against the live registry; the recipe's values are merged over the server's effective settings, nothing is applied if any entry fails, and the application is recorded.
- **Profile pages.** Document schema (12 column grid, nine block types, closed lists for font, cursor, overlay, border and pattern, palette with a 4.5:1 contrast rule, https links only, 32 KiB), `/u/:username` renderer in a scoped container, compact rendering in the profile popout, editor at `/profile-page/edit` (pointer drag and corner resize, the same from the keyboard, per-block forms, style panel, approved-layout picker, preview with the real renderer), guestbook (friends only, owner moderation, rate limit, block-aware).
- **Web.** Theme Store in Appearance (preview on the whole interface, apply to all sections or one; `useAppliedTheme(prefs, section)` lays tokens over the built-in theme after re-checking every key and value), Server Settings > Extensions, Developer Portal > Packages with the creator guide, Review Queues > Packages, API reference (read from the server's own OpenAPI document) and Gateway reference (generated from docs/05 by `scripts/lib/gateway-docs.mjs`, drift-tested). i18n fragments `p15-devplat-1.ts` to `-3.ts`; CSS with logical properties only.
- **`sr-cli package validate`** now validates every kind; a theme folder keeps its detailed report.
- **Bots in voice, the MLS half (`sr-sdk`, feature `voice-e2ee`).** `voice::Identity`, `voice::E2eeCall` (the op 101 state machine ported from `apps/web/src/voice/e2ee/call.ts`), `voice::E2eeMedia` (key ring, encryptor that works only after `execute`, grace-window decryption). It is sans-IO and uses the same `sr-e2ee` crate as the browser. Tests run real MLS groups (create, join by key package and welcome, leave by external Remove proposal, a bad welcome) against a relay that behaves like the gateway's op 101 and round-trip frames.
- **Data export and account deletion** include the new tables (`activity.json`: profile page, guestbook written and received, packages, enabled packages); deletion removes guestbook entries and theme enablements and withdraws pending versions.

## Still open (and why)

- **DEV-BOT-007.** Joining a channel (op 4, `VOICE_SERVER_UPDATE`) and the WebRTC/RTP session with the SFU are not built. `str0m` is in the lockfile for the server, but a client needs ICE, DTLS-SRTP, Opus packetising and the SFU's own signalling (`sr-voice-proto`); that is a separate piece of work, so a bot has no way to hear or speak yet even though it can now be a correct MLS member.
- **EXT-PKG-006.** Adding the files of an emoji, sticker or sound pack to a server (the guild expression routes take no pack item) and a server-wide theme (the route accepts it; members' clients do not read it, so the page does not offer it).
- **Pack kinds.** Lottie stickers; restricted CSS in themes (the token model has none).
- **Profile pages.** Arbitrary background images and uploaded pictures (need the media pipeline; the background is a pattern or the person's own banner, gallery entries are links).
- **DEV-APP-007, DEV-SDK-003**: as above.

## Tests run

- `sr-packages`: 15 unit tests; Postgres suite `tests/db.rs` (5: review gating, enablements and theme replacement, updates, pack files, profile documents, pages, guestbook and rate limit, layouts) in a scratch database (`--include-ignored --test-threads=1`).
- `sr-api`: 2 new lib tests (recipe checks against the real module registry); `route_matrix` (5) and `openapi_names` (2).
- `sr-cli`: `package_validate` (7, 3 new). `sr-sdk --features voice-e2ee --test voice_e2ee` (4). `sr-db` (Postgres): `account_deletion`, `data_export`, `secret_columns`.
- `cargo fmt --check` and `cargo clippy -D warnings --all-targets` on `sr-packages sr-api sr-cli sr-db sr-server`, and on `sr-sdk --features voice-e2ee`; `cargo deny --offline check bans licenses sources`.
- Web: `tsc`, `eslint .`, `prettier --check src`; the whole vitest suite with `--maxWorkers=2` (2,221 of 2,223 pass; the two failures, `src/test/vitestVersion.test.ts` and the `.rejects.toThrow` case in `src/review/__tests__/uploadManager.test.ts`, are the known vitest 2.x versus 3 mix-up of this worktree's runner and touch no file of mine); `vite build` and `scripts/check-bundle-budget.mjs` (lounge 407.3 of 600 KB, settings 281.0 of 600, developers 226.7 of 400, entrance 223.3 of 300).
- No Playwright, no server started (ports are shared). The upload route was tested with MSW and the services with Postgres; a real browser drag on the editor grid was not exercised (jsdom has no layout: the pure grid arithmetic and the keyboard path are tested).

## Decisions and owner questions

1. **Packages are reviewed through their own state on `package_versions`, not as `sr-review` submission kinds** (EXT-PKG-003 says "REV pipeline"). They appear in Review Queues > Packages with the same permission (`REVIEW_PACKAGES`) and the staff access log, as Activities did in p14. Folding them into `sr-review` (timeline, leases, `#review-queue` cards) is a later step. Confirm.
2. **Song blocks are not inline embeds and gallery pictures are links.** The app's CSP is `frame-src 'self'` and `img-src 'self'`; an embed or a remote picture would tell YouTube (or any host) who looks at a profile. A song asks once and opens its provider in a new tab. Allowing embeds means widening `frame-src` for a few providers (`csp.ts` and `Caddyfile.prod` change together). Owner decision.
3. **No arbitrary background image.** Pattern or the person's own banner only.
4. **Updates need fresh review (D-064)** is implemented as: an enablement stays on the version chosen; a newer approved version shows "Update available" and moves only when the person updates.
5. **A recipe cannot name channels or roles** (shared between servers); settings it omits keep their current value; the recipe's values are written as explicit overrides of the module.
6. **Guestbook:** 500 characters, 5 entries a minute per author, 500 entries per page, friends only, blocked people see nothing. Confirm the limits.
7. **Slugs are global across kinds** (a theme's slug is its `data-theme` name); first submitter owns it.
8. **Voice for bots.** `voice-e2ee` is a Cargo feature because it pulls in `openmls`; say if it should be on by default, and whether the TypeScript SDK should get the same through the existing WASM crate (`sr-e2ee-wasm`) or wait for the transport.
9. **Same-origin themes:** a store theme is applied as inline custom properties; its CSS (also stored) is not served. If staff later want `<style>` delivery, CSP `style-src` would need a hash or nonce.

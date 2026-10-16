# Handoff: feat/p14-devplat (developer platform: Activities first slice)

Branch `feat/p14-devplat`, based on `integration/phase14` (= main). Migration `0930`. Local commits only. Spec: [developer-platform.md](../../specs/developer-platform.md) section 3.10. Inventory: [17-developer-platform.md](../../inventory/17-developer-platform.md). D-185, D-186, D-204 to D-206, D-220, D-234 to D-237 are untouched.

## Audit result

| ID | Found | Result |
|---|---|---|
| DEV-ACT-001 | nothing (only the opt-in Caddy snippet, which served static files) | built as a first slice, ticked with limits |
| DEV-ACT-002 | nothing | built, ticked |
| DEV-ACT-003 | nothing | SDK and host built; `setActivity` and voice events missing, stays open |
| DEV-ACT-004 | nothing | call bar button, shelf, iframe panel built; text-channel launch and picture-in-picture missing, stays open |
| DEV-PORT-003 | only the Activities tab was open | built, ticked |
| DEV-BOT-007, DEV-APP-007, DEV-PORT-004, DEV-SDK-003, EXT-* | not built | untouched, reasons below |

## Done

- **`crates/sr-activities`** (new): `rules.rs` (entry URL and mapping validation, request path to target mapping with traversal refusals, activity host parsing), `service.rs` (settings with review state, shelf, instances, participants, events), `ports.rs` (channel permissions, user-scoped events). Migration `0930_activities.sql`: `application_activities`, `activity_instances` (one running per channel, partial unique index), `activity_participants` (FK to `accounts`, cascade).
- **Review model.** `draft -> pending (owner submits; the server fetches the entry document and pins its SHA-256) -> approved | rejected (staff with REVIEW_APPS)`. Any change to the entry URL or mappings returns the Activity to `draft` and clears the approved hash, so the proxy stops serving until it is approved again. Staff decide in Review Queues, Apps (new "Activities waiting for review" card) or through `POST /admin/developers/activities/{id}/decision`; the decision lands in the staff access log.
- **Proxy (`routes/activities/proxy.rs`).** Outermost layer in `sr-server/src/app.rs` (so gateway, CDN and SFU routes also answer an activity host with nothing). Host `<application-id>.activities.<public host>`; 404 unless `[activities] enabled`, GET/HEAD only, entry document served only on a hash match, URL mappings, `SafeClient` fetches (new `sr-server/src/activities.rs`), strict CSP with `frame-ancestors` = the app shell origin, nosniff, no referrer, Permissions-Policy, no application headers or cookies relayed.
- **API.** Owner: `GET|PUT /applications/{id}/activity`, `POST .../activity/submit`. Staff: `GET /admin/developers/activities`, `POST .../{application_id}/decision`. Members: `GET /activities` (shelf), `GET|POST /channels/{id}/activity`, `GET|DELETE /activity-instances/{id}`, `POST .../join`, `POST .../leave`. Gateway `SR_ACTIVITY_INSTANCE_UPDATE` (documented in doc 05). Route matrix rows (no `?` left; the member routes are `membership`, the permission check is in the service), OpenAPI, api-client schema and MSW handlers regenerated.
- **Config.** `[activities] enabled` (default false) in `sr-core` config. The Caddy snippet now reverse-proxies to sr-server instead of serving static files (`deploy_files.rs` test updated).
- **`packages/embedded-app-sdk`** (new workspace package): protocol (`protocol.ts`, documented at the top), `SideRoomSDK` with `ready`, `commands.{authorize, authenticate, getChannel, getInstanceConnectedParticipants, openExternalLink}` and `subscribe`; 6 tests.
- **Web (`apps/web/src/activities/`)**: `host.ts` (pure RPC host: handshake, origin check, scope whitelist `identify`/`guilds`, authorize through the existing `POST /oauth2/authorize` with `redirect_uri = <origin>/`, authenticate through `GET /oauth2/@me` without the session cookie and only for this application's tokens, http(s)-only links after a confirmation), `ActivityFrame` (iframe `sandbox="allow-scripts allow-same-origin allow-forms allow-popups"`, `allow="fullscreen"`, `referrerpolicy="no-referrer"`, source-window check, consent dialogs), `ActivityPanel` (above the call stage in `VoiceChannelView`), `ActivityLauncher` (shelf), `ActivityButton` (call bar, hidden unless the server answers `enabled`), portal tab `developers/tabs/Activities.tsx`, admin queue card. i18n `p14-devplat-1.ts` and `-2.ts`, CSS `activities.css` (logical properties).

## Still open (and why)

- **DEV-ACT-001 hardening.** The hash pins the entry document only. Scripts it loads are not pinned, so a changed script is not detected until the entry document changes. A real bundle hash needs an upload or a manifest of file hashes (owner decision, below). POST/other methods through URL mappings are not proxied (GET/HEAD only).
- **DEV-ACT-003**: `setActivity` (needs a presence extension for third-party activities) and voice-state events (needs the voice session events exposed to the client host).
- **DEV-ACT-004**: launching from a text channel (D-065 says voice/text) and picture-in-picture while navigating (the panel lives in the voice channel view and unmounts with it).
- **Participants are not tied to the voice session.** Joining and leaving are explicit; leaving the call does not remove you from the Activity. A sweep driven by voice-state events would fix it.
- **DEV-SDK-003**: needs discord.js and serenity (downloads). Left as the owner's CI action; no workflow added.
- **DEV-BOT-007** (MLS voice), **DEV-APP-007 user installs** (owner decision), **DEV-PORT-004** (generating portal reference pages from OpenAPI is its own pipeline), **EXT-PKG/PRF-*** (large, outside this slice).

## Tests run

- `sr-activities`: 4 unit tests, 2 Postgres tests (own scratch DB, `--include-ignored --test-threads=1`).
- `sr-api`: whole package (577 lib tests incl. 5 new Activities tests, route matrix 5, other targets), `sr-core` (40 + deploy files), `sr-db` `secret_columns`, `account_deletion`, `data_export`.
- `cargo fmt --check`; `cargo clippy -D warnings --all-targets` on `sr-activities sr-api sr-server sr-core`.
- Web: `tsc`, `eslint`, `prettier --check`, vitest for `src/activities` (15), `src/developers`, `src/styles`, `src/lounge/voice`, `src/watchparty`, `src/i18n`, `src/admin/__tests__/p10.test.tsx` (22 incl. the new Activities approval); `vite build` and bundle budget (lounge 388.9 of 600 KB, developers 223.2 of 400). `packages/embedded-app-sdk` typecheck, lint, 6 tests.
- No Playwright, no server started (ports are shared). The proxy was tested with a fake fetcher; the real `SafeClient` path and a real browser iframe were not exercised.

## Decisions and owner questions

1. **Review is per Activity, not per app version.** doc 11 says activity code needs re-review per version; I kept it a separate state on `application_activities` (cleared by any edit) so it does not entangle the app-version pipeline. Confirm, or fold it into the version manifest.
2. **Pinned hash covers the entry document only** (see above). Confirm that is acceptable for v1, or choose: developer uploads a bundle (zip) that sr-server hosts, or a manifest of file hashes.
3. **Redirect URI.** `authorize` returns the code to `<origin>/`; the developer must register that URI on the OAuth2 tab (the Activities tab shows it with a copy button). Discord uses a fixed placeholder; I did not auto-add the URI.
4. **Scopes an Activity may request:** `identify` and `guilds` only (no `email`, no bot scopes).
5. **Who may start one:** `CONNECT` plus `USE_EMBEDDED_ACTIVITIES` in the voice channel (bit already in `Permissions`), one Activity per channel, any approved non-suspended application (no install needed, as on Discord).
6. **Config key `[activities] enabled`** follows the name doc 12 and OQ-XC-3 already use. Operators must switch it on together with `VITE_ACTIVITIES_ENABLED`, the wildcard Caddy snippet and the CSP (the snippet now says so).
7. The first Activities commit of this branch (`activities API ... proxy`) lacks the Co-Authored-By trailer; the others have it.

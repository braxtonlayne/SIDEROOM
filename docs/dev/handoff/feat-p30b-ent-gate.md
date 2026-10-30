# Handoff: feat/p30b-ent-gate (SEC-ENT-003, D-385)

Branch `feat/p30b-ent-gate`, base `integration/phase30b`. Design: `docs/dev/entrance-isolation.md` (binding), section 3 above all.

## Resume here
All of SEC-ENT-003 is committed and green as far as it can be proven without Postgres, Caddy and the built dist. What is left is integration: ent-build adopts the Caddyfile marker block in `csp.ts` / `csp.test.ts`, then the integration run (`web_gate.rs` against the real `dist`, `prod-smoke`) proves the rest. See "Still open".

## Done
- **`[web]` config** (`crates/sr-core/src/config.rs`): `root` (`SR_WEB__ROOT`, unset = no documents) and `serve_public` (default false).
- **`crates/sr-server/src/web.rs`** (new, wired in `app.rs` after the SFU merge and before the activity-host wrapper): `WebGate::from_config/open/install`.
  - Session before path (the API's expiry policy; bot and bearer credentials count as anonymous).
  - Anonymous: `GET /app`, `/app/` and any navigation (`Sec-Fetch-Mode: navigate` or `Accept` preferring HTML) to a non-public path is `302 /login?next=<path+query>` (`next` is dropped when unsafe: `//`, backslash, control character, `/@`, over 2 KB; encoded for a query, `/` kept), `no-store`, no body. `/app/*` is `401`, empty, `no-store`, identical headers for a present and an absent file. Any other non-navigation request to an unknown path is `401`. Other methods `405` (`Allow: GET, HEAD`).
  - Signed in: `app.html` with `private, no-store`, `Vary: Cookie`, the app CSP read verbatim from `app/csp.txt`, `Reporting-Endpoints`, `X-Robots-Tag: noindex, nofollow`. `/app/*` files: hashed names `private, max-age=31536000, immutable`, others `private, no-cache`, `Vary: Cookie`; a missing file is `404`.
  - Path hygiene: no empty, dotted, `..` segment, no backslash, colon or NUL, so nothing escapes `app/`; a dotted name (`/entrance/.vite/`) is never served, even under a public prefix.
  - Per-IP bucket for the gate's `401`s (240 per minute per address, then `429` with `Retry-After`), separate from the API limiter (uses the same `client_ip`/`trust_forwarded_for`).
  - Start-up: refuses when the root is not a directory or `app/csp.txt` is missing or not one header line; with `serve_public`, also when `surfaces.json` is missing.
  - `serve_public`: the shells from `surfaces.json` (`bundle` surfaces that are not `reserved`), `publicFiles` and the `error` surface's files, with today's Caddy cache rules (`public, max-age=31536000, immutable` for `/entrance/*` and `/fonts/*`, `no-cache` for shells, `sw.js`, the manifest, offline files), the public CSP (`PUBLIC_CSP`, no `wasm-unsafe-eval`, no `blob:`, no `frame-src`), the no-script CSP for the error pages, noindex on everything except `/` (D-384). Without it, a public path that reaches the server is a plain `404` (Caddy serves those).
  - `GET /house-notes` (always, Caddy proxies it): `legal/house-notes.<locale>.html`, `?lang=` then `Accept-Language` (weights, primary-subtag fallback, only `[A-Za-z0-9-]` tags so a tag is never a path), English fallback; `{{operator_country}}` and `{{operator_contact}}` replaced in one pass and HTML-escaped (a value containing a placeholder is never re-expanded); `no-cache`, the no-script CSP.
  - Not done in Rust: compression and range requests (Caddy's `encode` does the first; the second is unneeded). Files stream in 64 KiB chunks (the 32 MB ffmpeg wasm is not buffered).
  - `/share` GET is routed to the gate explicitly because the API router owns `POST /share` and axum would otherwise answer `405` to a GET (an oracle for that one path).
- **API oracle fixes** (`crates/sr-api/src/router.rs`, `openapi.rs`): `not_found` is now `pub` and answers `401` to an anonymous caller for any path under `/api/` (real or unknown), `404` JSON to a caller with a principal; outside `/api/` it is the same `404` as before. `GET /api/v1/openapi.json` needs a principal (`401`). `GET /developers/docs/api` without a principal is `302 /login?next=/developers/docs/api` (`no-store`). `sr-server openapi` (CLI) is unchanged. `sdk-compat.yml` builds the document with the CLI, so it is not affected. Tests updated: `api.rs`, `guilds.rs`.
- **Logout**: `POST /sr/auth/logout` and `logout-all` add `Clear-Site-Data: "cache", "storage"` (`routes/auth/sessions.rs`, asserted in `auth/tests.rs`).
- **`activities::proxy` test** (`crates/sr-api/src/routes/activities/tests.rs`, because the proxy and its rig live there, not in `sr-server/src/activities.rs`): a response from the activity host never names `/app/`, `/entrance/`, `app.html` or `csp.txt`.
- **Preflight row `web`** (`crates/sr-preflight`: `Settings.web_root`, `checks::web_root`, `CHECK_IDS` gains `web` at the end; `docs/dev/server.md`; `prod-smoke.sh` allows pass or warn for it from the host-side CLI): warn when `web.root` is unset, fail when `app.html` or `app/csp.txt` is missing, pass otherwise. The roadmap said `crates/sr-cli`; the checks live in `sr-preflight`, which `sr-cli` and `POST /admin/system/preflight` share.
- **Deployment**: `deploy/compose/caddy/Caddyfile.prod` (public matchers per surface between `# surfaces:begin` / `# surfaces:end`, `@dotfiles` 404, `@legal` and the default `handle` are `reverse_proxy`, no `try_files`, `handle_errors` serving `404.html` / `50x.html`); `deploy/compose/prod.yml` (the server container gets `SR_WEB__ROOT=/srv/web` and the read-only `SR_WEB_DIR` mount); `deploy/systemd/sr-server.service` (`Environment=SR_WEB__ROOT=/opt/sideroom/web`, overridable in `server.env`); `INSTALL.md` step 6, `UPGRADE.md` (a new release-notes section with the owner's one-time steps and step 5), `FAQ.md`.

## Contract for the other agents
- **ent-build**: `renderCaddyfile` in `apps/web/src/security/csp.ts` must now rewrite the *block between `# surfaces:begin` and `# surfaces:end`* of `Caddyfile.prod` (the matchers, one `handle` per bundle surface in `public-surfaces.json` order with `rewrite * /<shell>`, the `@legal`, `@publicFiles` blocks) and the three identical `header Content-Security-Policy "<public policy>"` lines inside it. The app policy is no longer in the Caddyfile; `csp.test.ts` must pin the committed block (it currently still looks for the old single app-policy line and will fail until ent-build updates it). The public policy string is also `PUBLIC_CSP` in `crates/sr-server/src/web.rs`; `csp.test.ts` should pin the two together (read the Rust constant) or the Rust test below does it from the TS side later.
- **ent-build / dist**: the server reads `dist/app.html`, `dist/app/csp.txt` (one line), `dist/surfaces.json` (the copy of `public-surfaces.json`; keys `surfaces[].{kind,shell,paths,files,reserved}` and `publicFiles`), `dist/legal/house-notes.<locale>.html` (placeholders `{{operator_country}}`, `{{operator_contact}}`) and serves `dist/<shell>` for each non-reserved bundle surface. A new locale needs no server change.
- **ent-harness**: `SR_E2E_WEB=server` = `sr-server` with `SR_WEB__ROOT=apps/web/dist SR_WEB__SERVE_PUBLIC=true`. Status table is pinned in `crates/sr-server/tests/web_gate.rs`; `curl`-style requests need `Accept: text/html` or `Sec-Fetch-Mode: navigate` to get the `302` (otherwise an unknown path is `401`).

## Still open
- `route_matrix.tsv` has no row type for undocumented routes (the router-literal check only lists their paths), so `/app`, `/app/{*path}` (which live in `sr-server`, outside the matrix's source tree) and `/house-notes` have no row; `/api/v1/openapi.json` and `/developers/docs/api` were already `#undocumented`. The gate's behaviour is pinned by `web_gate.rs` and the `api.rs` tests instead. `authz_matrix.rs` (Postgres) was not touched.
- Postgres suites were not run: `%TEMP%\sr_pg_pw.txt` is missing. CI runs them.
- `Caddyfile.prod` was not run through Caddy (not installed here); `prod-smoke` in CI exercises it. Points to watch there: `handle_errors` keeps the original status code with `file_server`, and the `path_regexp` for dotted names.
- Bearer tokens cannot read `openapi.json` while `enforce_credential_policy` is on (`auth::policy` lists no bearer scope for it); the design says "bot/bearer principal". Adding it is a one-line policy change plus a matrix cell, left for the owner of `auth/policy.rs`.

## Tests run
(`CARGO_TARGET_DIR=.../p30b-ent-gate-target`, `-j 2`, one cargo command at a time.)
- `cargo test -p sr-server --test web_gate`: 18 passed (the status table with and without a session, present and absent files, `next` cases, bot, bearer, expired, unknown and revoked sessions, path traversal, `serve_public` on and off, legal renderer escaping and locale fallback, `csp.txt` and `surfaces.json` refusals, the `401` bucket, the API oracle).
- `cargo test -p sr-server --lib web::`: 8 passed.
- `cargo test -p sr-api`: all passed (737 lib tests incl. the activity-host test and the logout header assertions, route_matrix, openapi_names, api.rs with the new anonymous and principal cases, guilds.rs).
- `cargo test -p sr-preflight`: 23 passed (new `web_root_states`, `check_ids_are_stable` with `web`); the Postgres test is ignored without a database.
- `cargo test -p sr-core`: 43 passed.
- `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` on sr-core, sr-api, sr-preflight, sr-server: clean.
- Not run: Postgres suites (`authz_matrix`, `immutable_rules`, sr-preflight `pg`): `sr_pg_pw.txt` is missing. Caddy, Docker and the web build were not run. The 3 other Caddyfile/CSP tests in `apps/web` (`csp.test.ts`) will fail on this branch alone until ent-build adopts the marker block (see "Contract").

## Open owner questions
1. Should a bearer (OAuth2) token be allowed to fetch `openapi.json`? (Today: session or bot only.)
2. The `429` bucket figure (240 `401`s per address per minute) is a conservative default; it is not an operator setting yet.

## Owner's hosting steps (also in `deploy/UPGRADE.md`, "the web app is served by the server")
1. Compose: take the new `compose/prod.yml`; `SR_WEB_DIR` (already set) is now mounted into `sr-server` too and `SR_WEB__ROOT=/srv/web` is set. Plain binary: set `SR_WEB__ROOT=/opt/sideroom/web` in `server.env` (the unit already does).
2. Re-render the Caddyfile (`node deploy/scripts/render-caddyfile.mjs --public-url https://<domain> ...`) or take the release's file; `$dc up -d --force-recreate caddy`.
3. Unpack the web build before restarting the server (the server refuses to start without `app/csp.txt`); `sr-cli preflight` shows the `web` row.
4. Check: signed out, `/channels` is `302 /login?next=/channels`, `/app/anything.js` is an empty `401`; signed in, the app opens.

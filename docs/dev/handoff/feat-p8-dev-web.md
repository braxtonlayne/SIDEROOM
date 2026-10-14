# Handoff: feat/p8-dev-web (developer portal, consent screen, Authorized Apps)

Frontend only, coded against `docs/specs/developer-platform.md` section 3 ("HTTP API as built") on `feat/p8-dev-be` (read at the start of the work and re-checked at the end: unchanged). The backend was not available to run against; everything is tested with MSW.

## Done

- Route group `/developers` (lazy, own chunk `DevelopersRoot`, budget entry `developers` at 400 KB gzip; measured 204 KB): overview, applications list, application detail, documentation hub. Replaces the stub page.
- Overview and developer application (DEV-PORT-001): `GET /developers/status`, `POST /developers/apply` form with pending, approved and rejected states.
- Applications list and New application (DEV-PORT-002). Creating shows the client secret and bot token once in a reveal dialog.
- Application detail tabs (DEV-PORT-003, partly):
  - General: name, description, icon (data URI, 2 MB limit), privacy and terms URLs (https), public bot toggle, delete (typed name plus password).
  - OAuth2: client id, secret hint, secret reset (password re-confirmation, reveal once), redirect URI editor (validated like the server: https or http on localhost, no fragment, no wildcard, at most 10), scopes picker, permission calculator (the server settings permission catalog), authorize URL builder.
  - Bot: rename, token reset (password, reveal once), privileged intents toggles with review status (off, waiting, approved).
  - Installation: invite link builder starting from the app's default install params, "Save as default install settings", installs list, notices for test-server rule (error 900120) and private bots.
  - Commands: a command builder (see decisions).
  - Review: submit a version with a note, version list with state, reviewer message, needs-review reasons and manifest diff, withdraw.
- Documentation hub (DEV-PORT-004, partly): getting started, authentication, gateway, REST, rate limits and status, interactions; curl and TypeScript samples with copy buttons. Content lives in `developers/docsContent.ts` and follows the spec (gateway-only interactions, no sharding, PKCE, rate limits).
- OAuth2 consent screen `/oauth2/authorize` (DEV-APP-008) outside the portal chrome, plus `/oauth2/authorized` landing: app, description, release warning, scopes, server picker with `can_install` and reasons, permissions requested, Administrator warning, Authorize and Cancel, error states (400 from the server is shown and never redirected, unknown app, missing client_id or scope, signed-out goes to login and back). The redirect target from the server is only followed when it matches the registered redirect URI (or is an in-app path).
- Settings > Authorized Apps (LNG-USET-021): `GET /oauth2/tokens`, Deauthorize with confirm (`DELETE /oauth2/tokens/{id}`); keeps the honest "not available yet" state when the endpoint answers 404, 405 or 501.
- Secrets: only held in component state while the reveal dialog is open, dropped on close, never written to storage or logged. Reset and delete use a password confirm dialog (spec 3.2 requires `{password}`; `900004` is shown as "That password is not right.").
- Inventory ticked: DEV-APP-008, DEV-PORT-001, DEV-PORT-002, LNG-USET-021. DEV-PORT-003 and DEV-PORT-004 annotated as partial and left unticked.

## Not done (and why)

- Slash command list, edit and delete: the server lets only the bot token manage commands (spec 3.5), so a signed-in session cannot read or write them. Not faked.
- Interactions endpoint URL / Ed25519: the backend has none (spec section 2).
- Emojis, Webhooks, Rich Presence assets, App Testers, Activities tabs: not built on the backend. Team and owner transfer: no teams on the backend.
- Application audit log view (`GET /applications/{id}/audit-log`) and staff-side review UI (the admin Review queues belong to the admin section).
- API docs generated from OpenAPI: the hub is hand-written. The server-rendered docs path `/developers/docs/api` stays a server route (the service worker already bypasses it); no hub page uses the id `api`.
- Mobile layout is CSS (wrapping bar, scrolling nav, stacked doc nav); no visual check in a real browser was run, and no e2e (shared ports).

## Decisions

- Since every application has its own bot (created with the app), there is no "create bot" step; the Bot tab renames and resets.
- The Commands tab is a builder that validates against the server rules and prints the JSON body and a curl call for the bot token, instead of a manager.
- Redirect URIs with custom schemes are refused by the server (spec section 2), so the editor refuses them too.
- Grants endpoint is `/oauth2/tokens` (spec 3.3), not `/oauth2/grants`.
- `search.ts` holds the route search parsers so the router imports only that, not the page modules.
- Permissions integer uses `settings/server/permissions.ts` (same bit numbers as the server crate).
- `routes/pages.tsx` is now unused (its only export used was `Developers`); left in place to keep shared-file edits small.

## Tests and gates

- New: 83 tests under `apps/web/src/developers/__tests__` (oauth helpers 25, portal 44, consent 14 including axe on the main states) and 4 in `settings/user/__tests__/authorizedApps.test.tsx`; updated `prefsSections` (404 handler) and `router.test` (heading).
- Whole web suite: 89 files, 1155 tests passing. Typecheck, eslint, prettier, build and `node scripts/check-bundle-budget.mjs` all pass.
- Not run: Rust, e2e.

## Reconcile later

- If the backend adds session access to application commands, replace the builder with a list in `tabs/Commands.tsx` and add the calls to `developers/api.ts`.
- If `GET /oauth2/authorize` guild entries gain more `reason` values, extend `REASON` in `Authorize.tsx`.

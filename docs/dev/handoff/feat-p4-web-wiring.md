# Handoff: feat/p4-web-wiring

Branch `feat/p4-web-wiring` (based on `integration/phase3`), worktree `SIDEROOM_wt/p4-web-wiring`. Nothing pushed.

## Done

| Item | What changed | Where |
|---|---|---|
| F-11 web | The Create a Server wizard lists `GET /guilds/templates/builtin` (name, description) and creates with one `POST /guilds {name, template}`. A failed list shows a note and "Create My Own" still works. `serverTemplates.ts` and the sequential channel replay are deleted. | `lounge/AddServerDialog.tsx`, `store/{rest,store}.ts` |
| F-12 web | READY `private_channels` are read with full `recipients`; request DMs are kept in a new store slice `messageRequestDms` (READY, CHANNEL_CREATE/DELETE) which seeds the social store. `loadDms` and the boot `GET /users/@me/channels` are gone; the Message Requests page no longer fetches. | `store/reducer.ts`, `lounge/shell/{social,useShellBoot,MessageRequestsPage}` |
| F-12 backend gap | READY lacked `is_message_request`; `build_ready_extras` now sets it on a Pending request like `GET /users/@me/channels`. No OpenAPI change (READY is not in the spec). Doc 05 §6.4 mentions it. | `crates/sr-server/src/gateway_pg.rs` |
| F-24 | My Account gets "Change email": new email + password, then (2FA on) a code step (6 digits is sent as `totp`, anything else as `recovery`). If the server answers `errors.mfa` (fresh-session rule, TOTP added elsewhere) the dialog moves to the code step. `/email-change/cancel?token=` page posts `POST /sr/auth/email/cancel` once. `/verify-email/{token}` copy covers the change case. | `settings/user/sections/{Account,ChangeEmail}.tsx`, `settings/user/api.ts`, `routes/entrance/{EmailChangeCancel,VerifyEmail}.tsx`, `api/entrance.ts`, `router.tsx` |
| Prune copy | `LAST_SEEN_TRACKED` is true and the rule reads "Members who have not been online for N days". | `settings/server/sections/PruneDialog.tsx` |
| Remote images setting | `load_remote_images` (default off) in the settings blob; a switch under Privacy & Safety; `EmbedView` skips the "Load image" click when on (still `no-referrer`; proxy_url unchanged; video embeds stay gated). | `lounge/msg/{remoteImages.ts,Embeds.tsx}`, `settings/user/sections/Privacy.tsx` |

Follow-ups ticked: F-11 and F-12 notes (web side), F-24. LNG-SSET-045 caveat reworded.

## Not done

- LNG-SSET-045 webhook avatar: the backend does not support it. `CreateWebhookBody`/`ModifyWebhookBody` have no avatar field and `sr-storage::MediaKind` has no webhook kind (the row has `avatar_key`, nothing writes it). Needs a `WebhookAvatar` media kind plus `avatar` data URI on create/modify, then the UI.
- The email-change dialog has no passkey step (the backend only knows TOTP and recovery codes).
- The Prune count preview text for roles is unchanged.

## Tests

- web: 473 passing in 36 files (451 before). New: wizard template tests (list, single POST, failed list, axe), READY recipients/requests test, change-email (4, with axe), cancel page (3, axe), verify copy, remote-images setting (2 in Privacy, 3 in embeds), prune wording.
- Gates: web typecheck, eslint, vitest, prettier, `vite build` clean; `cargo fmt --check`, `clippy -p sr-server --all-targets -D warnings`, `cargo test -p sr-server` (20 passed) clean. New ignored Postgres test `gateway_pg::tests::ready_private_channels_flag_message_requests` passed on a throwaway DB (dropped).
- Not run: Playwright e2e, full workspace cargo test, `cargo deny` (no dependency change).

## Decisions / notes

1. `messageRequestDms` is store state (not only social state) because the shell can mount after READY; if you would rather keep it in the social store only, READY must be observed earlier.
2. Request DMs ignored through the API are removed locally; the server does not need to send CHANNEL_DELETE for that.
3. The cancel page posts on load (the token is single use and a link scanner does not run JS). A confirm button would be safer against prefetchers that do; say if you want it.
4. "Discord's wording" for prune is taken as "have not been online for N days".

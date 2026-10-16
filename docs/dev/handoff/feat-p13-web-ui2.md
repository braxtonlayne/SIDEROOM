# Handoff: feat/p13-web-ui2

Branch `feat/p13-web-ui2`, based on `feat/p13-settings-admin`. Web only (no cargo, no Rust change). It wires the backend contracts
from `docs/05-api-and-gateway.md` section 5.3 into the web app. The generated client was already current; the pages use the
hand-written `apiFetch` wrappers the rest of the app uses, with types copied from `packages/api-client/openapi.json`.

## Done

| Item | What (files) |
| --- | --- |
| LNG-USET-012 | `settings/user/sections/SafetyCards.tsx` `SensitiveMediaCard`: show / blur / block for friends, others and servers, saved on change with a partial `PATCH /users/@me/privacy`, rolled back on error. |
| LNG-USET-013 | Same file, `DmSafetyCard`: DM spam filter (off / non_friends / all) and a per-server override list (default / allow / do not allow, `null` puts it back). Message requests and server DMs switches were already in `Privacy.tsx`. |
| LNG-USET-015 | `AgeConfirmationCard`: shows the stored state from `GET /users/@me/age-confirmation`, "Confirm again" posts `/sr/age-gate`, "Reset my answer" asks for the password (`PasswordPrompt`) and calls `DELETE`. `setLocalAgeAnswer` in `lounge/AgeGate.tsx` keeps this device's cached answer in step, otherwise the Lounge interstitial would not ask again after a reset. |
| Notification keywords | `settings/user/sections/KeywordsCard.tsx` (Notifications section): add / remove over `PUT /users/@me/notification-keywords`, client checks mirror the server (50 words, 100 chars, a letter or digit, no duplicates). `lounge/keywords.ts` fetches the list once into the store (`notificationKeywords`, `SR_NOTIFICATION_KEYWORDS_UPDATE` replaces it) and `MessageList` highlights a message from someone else that contains a keyword as a whole word or phrase, like a mention (`data-mention`). |
| ENT-UI-010, ENT-UI-025 | `routes/entrance/entranceCopy.tsx`: loader for `GET /sr/entrance/copy` (module cache, failure keeps the built-in text) and a small restricted markdown renderer (headings, lists, bold, italic, code, http(s) and site-relative links; raw HTML is text; headings get slug ids so `/house-notes#privacy` keeps working). `/no-invitation` replaces its body, `/house-notes` replaces everything under the h1, the landing page shows `landing-note` under the invite box. Firstpaint rules kept: the entrance modules add no `t()` keys (the static scan test passes), no catalog change for the core chunk. |
| ADM-PLT-003 | Admin section `entrance` ("Entrance pages", `MANAGE_FEATURES`): three blocks with a byte counter (16 KiB), preview, save (optional reason) and reset to default; PUT and DELETE go through `withReauth`. Saving updates the in-memory copy so a visitor page in the same tab changes at once. |
| ADM-PPL-001 | Accounts search has a "Search by" selector: name/email/id, Discord id, IP address (`discord_id=` / `ip=`). |
| ADM-PPL-002 | `sections/AccountExtras.tsx` in the account drawer: sessions and servers lists (loaded on a button press, since each read is written to the access log), invite freeze / unfreeze, and "Unlink from Person". |
| ADM-PPL-004 | `PersonDrawer.tsx`: edit notes, link an account by id, suspicion log. 409s from link / unlink show the server message. |
| ADM-GRAPH-004 | Freeze / unfreeze invites on the graph node panel (`InviteGraph.tsx`). |
| ADM-COM-002 | Admin section `servers` (`MANAGE_GUILDS_GLOBAL`): search, metadata drawer, transfer ownership (account id), delete (type the server name, reason). Staff Server and already deleted servers have the buttons disabled. |
| LNG-EXP-011 | `lounge/msg/CustomEmoji.tsx`: a custom emoji in a message is drawn from `GET /sr/emojis/{id}/url` (so emoji from servers the viewer is not in render) and is a button; the popout (`GET /sr/emojis/{id}`) shows the name, the source server, "Join server" when `listing` is set and the viewer is not a member (`POST /directory/{listing}/join`), or "private server". `packages/markdown-react` gained an optional `customEmoji` resolver for this (additive). Only the message list passes it; embeds, forwards and the composer preview keep the plain `:name:` fallback. |
| D-233 | `Composer.addFiles` drops files larger than `sr_max_upload_bytes` of the guild (`GET /guilds/{id}` through `loadGuild`) or DM channel (`GET /users/@me/channels`) and toasts the first name. Unknown limit: no client block, the 413 stays the real check. |
| D-231 | Burst reaction shim needs no UI; nothing built. |

Inventory ticked with notes: 07 (LNG-USET-012, 013, 015), 02 (ENT-UI-010, 025), 10 (ADM-PPL-001, 002, 004, ADM-GRAPH-004,
ADM-COM-002), 05 (LNG-EXP-011). ADM-PLT-003 got a note and stays open (no hero image route). New i18n fragments:
`i18n/messages/en/p13-web-ui2-{1,2,3,4}.ts`.

## Still open

- ADM-PLT-003: hero image editing (no backend route).
- LNG-EXP-012 (burst picker variant): closed by D-231 (shim only); the box is left unticked, the owner can tick it as "won't build".
- LNG-CMP-005 / 007: the upload tray boxes are unchanged; only the oversize pre-check was added.
- The sensitive-media choice is stored but the message pipeline does not blur or block yet (backend, see the settings-admin handoff), so the setting has no visible effect on messages today. The card says what each option means.
- Custom emoji in embeds, forwarded messages and the composer preview are not clickable (no resolver passed there).
- The upload limit is only known after the guild is loaded; `GUILD_CREATE` / `GUILD_UPDATE` do not carry it (backend note D-233).
- Playwright was not run (shared ports).

## Tests run

- New: `settings/user/__tests__/p13Safety.test.tsx` (7), `routes/entrance/entranceCopy.test.tsx` (5), `admin/__tests__/p13entrance.test.tsx` (2),
  `admin/__tests__/p13people.test.tsx` (10), `lounge/__tests__/p13WebUi2.test.tsx` (7), `lounge/__tests__/keywords.test.ts` (2).
  `privacy.test.tsx` lost its "no media scanning" assertion.
- Gates: `tsc --noEmit`, `eslint .` (web and markdown-react), `prettier --check`, full `vitest run --maxWorkers=2`, `vite build` plus
  `scripts/check-bundle-budget.mjs`, `scripts/i18n-verify-diff.mjs` (MISSING 0). See the final report for the numbers.
- The wasm `dist` was copied from the main checkout (git-ignored).

## Decisions / notes

- Entrance markdown is rendered client-side with a tiny restricted renderer instead of the wasm parser, to keep the first-paint
  chunk free of the markdown engine; the server does not parse it either (contract in 5.3).
- Heading levels in operator copy: `#` and `##` become h2 (the page keeps its h1), deeper levels h3 and h4.
- Account sessions and servers are not auto-loaded: the sessions read is logged server side.

## Open owner questions

- None new. Carried over: should `sensitive_media` also apply to the gateway payloads (backend), and should the Entrance copy get a dedicated permission bit instead of `MANAGE_FEATURES`.

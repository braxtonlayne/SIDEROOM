# Handoff: feat/p11-p10-client (client side of the phase 10 backends)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p11-p10-client, branch feat/p11-p10-client (base integration/phase11). Web only (`apps/web`); no backend or API change.

## Done

| Item | What |
|---|---|
| Hide "Invite to Server" in the Staff Server (ADM-STF-003 client half) | When `guild.isStaff` the invite entry is gone from the rail context menu, the server dropdown, the channel context menu, the channel row button, the Server Welcome checklist and the Create Invite button in Server Settings > Invites. Tests in `lounge/__tests__/homeFlows.test.tsx`. |
| Appointment offer buttons (ADM-STF-004) | `lounge/msg/Components.tsx` + `staffActions.ts`: a button whose `custom_id` is `sr:offer-accept:<id>` / `sr:offer-decline:<id>` calls `POST /staff/offers/{id}/respond {accept}`; after an answer both buttons stay disabled, a refusal shows the server text in a toast and the buttons stay usable. `MessageItem` now passes `channelId` / `messageId`. Other `custom_id`s (apps) keep the old "interaction failed" behaviour. |
| Staff card buttons (ADM-STF-007) | Any other `sr:` id calls `POST /admin/card-actions {channel_id, message_id, custom_id}` and shows the returned message under the card. Tests `lounge/__tests__/staffCards.test.tsx`. |
| Revoke push (ADM-STF-005) | `SR_STAFF_ALERT {revoked: true}` closes `/admin` (`admin/useStaffRevoked.ts` in the shell) and clears the cached "is staff" flag in `lounge/shell/platformStaff.ts` (module-level `onDispatch`, so the Platform Admin entries vanish on every screen). |
| Re-auth modal (ADM-STF-009 client half) | `admin/reauth.ts` (`withReauth`, `registerReauthPrompt`) and `admin/ReauthHost.tsx` (mounted by `AdminShell`). Every call in `admin/api.ts` (`get` and `send`) goes through `withReauth`: a 403 with code 900004 opens the modal (authenticator code, recovery code or passkey via `POST /sr/auth/passkeys/verify/begin`), `POST /admin/reauth`, then the original call is retried once. Cancelling rethrows the original error; staff without any second factor see an "enrol first" notice (from `GET /admin/reauth` `enrolled: false`). Parallel refused calls share one dialog. So reset-2FA, appoint, set roles, revoke and the private reads are all covered with no per-screen code. |
| Private Content Viewer (ADM-COM-003) | `admin/sections/PrivateContent.tsx`, section `private-content` (needs `READ_PRIVATE_CONTENT`): by channel id, by account (its DMs) or by server (its channels); reason required and sent as `reason`; "Access is logged" notice; newest first with "Load older"; badges for deleted by author / removed by staff / edited; edit history in a details element; retention note from `deleted_retention_days`. |
| Announcement banner (ADM-PLT-009 client half, LNG-SHL-009 data) | `lounge/announcements/` exports `useActiveAnnouncements` (fetch at start, poll every 5 minutes, per-browser dismissals in localStorage, critical cannot be dismissed), `AnnouncementBanner` (self-fetching) and `AnnouncementList` (pure) via `index.ts`. Not mounted anywhere: the notices bar is owned by p11-shell, which should render `<AnnouncementBanner />` (or `AnnouncementList` with its own data). Tests `lounge/announcements/announcements.test.tsx`. |
| Server tag editor (LNG-SSET-011) | Server Settings section "Server Tag" (needs Manage Server): text 2 to 4 letters or digits, 16 badge glyphs (`lib/serverTag.ts`), colour picker, "Members can choose to display", live preview, save and remove. The "not available" tag line in Overview was removed. Test `settings/server/__tests__/tag.test.tsx`. |
| User tag picker (LNG-SSET-012 client half) | Card "Server tag" on User Settings > Profiles > Servers: lists the servers that have a tag with display allowed (one `GET /guilds/{id}/tag` per server, 404 = none), `GET/PUT /users/@me/primary-guild`, "No tag" clears. Test `settings/user/__tests__/serverTag.test.tsx`. |

New strings: `apps/web/src/i18n/messages/en/p11-p10client-1.ts`.

Inventory: ADM-COM-003 and LNG-SSET-011 ticked with notes; ADM-STF-010 note corrected; LNG-SSET-012 note extended. ADM-STF-011 was already ticked.

## Not done / still open

- "SIDE ROOM Staff" profile badge (ADM-STF-010 second half): no field exists. User and profile objects carry no staff flag, and `accounts.flags` bits 0/1 (system, app developer) and 8 to 11 (staff-granted badges) are not staff markers. Needs a backend flag (for example a public `staff` bit on the profile) first. Left as noted.
- The server tag is not yet drawn next to author names in chat or in the member list. The data (`primary_guild` on author and user objects) is already delivered; `TagPreview`/`.ss-tag-chip` and `lib/serverTag.ts` are ready to reuse. This is the chat renderer's area.
- The staff-card re-auth prompt only exists inside `/admin` (the `ReauthHost` is mounted by `AdminShell`). A card button pressed in the lounge that needs a second factor shows the server's refusal as a toast instead of a dialog. Cards that run today (ack, clear suspicion, retry job) may need it; moving `ReauthHost` into the app root would fix that, but it touches a file owned by another agent.
- No admin UI yet for sending offers (`POST /admin/staff/offers`), delegations, composing announcements or the new account actions (rename, change email, badges, force password reset). Only the re-auth wrapper covers them if a screen calls them. The Staff screen still uses the direct appoint route (owner question from the admin-be handoff: remove it?).
- Passkey re-auth was written to the documented body (`{ceremony_id, credential}` JSON string) but is not covered by a test (jsdom has no WebAuthn).
- Playwright was not run (shared ports).

## Tests run

- `pnpm`-free equivalents inside apps/web: `tsc --noEmit`, `eslint src`, `prettier --check src`: clean. `vite build` and `scripts/check-bundle-budget.mjs`: all budgets ok (admin 279 KB, settings 248 KB gzip).
- vitest (`--maxWorkers=2/3`) on `src/i18n src/admin src/lounge src/settings src/test`: 774 tests; only `shellFlows > mutes the server for an hour` failed, which is a cold-start timing flake that also fails on the untouched base commit (passes when the file runs alone). New tests: 5 + 3 + 3 + 2 (+2 in homeFlows) + 3 + 3 groups listed above.

## Decisions / owner questions

- 900004 handling is global in `admin/api.ts` rather than per screen, so any future admin call is covered automatically. It also means the wrong-password use of 900004 elsewhere is unaffected (those use `apiFetch` directly).
- Badge glyphs for ids 0 to 15 are my choice of 16 plain Unicode symbols (no icon font). Owner: replace with real artwork?
- Should critical announcements really be undismissable? Chosen so a platform outage notice cannot be hidden; easy to change in `canDismiss`.

# Wave 56 web builder (D-585, D-586)

Branch `fix/p56-web`. Web only: no Rust, no migrations, no API change.

## Done
- **SEC-AUD-055 (D-585)**: the Cinema comments list (`apps/web/src/cinema/Social.tsx`) gets a Reply button on every
  top-level live comment (not on replies: one level; not on a "Comment deleted" tombstone, which the API refuses) that
  opens an inline `ReplyForm` (textarea, "Contains spoilers", Post reply, Cancel) posting `parent_id`. On success the
  comments queries are invalidated (reply count and the replies list) and the replies open. `postComment` takes
  `parent_id`. Strings in `i18n/messages/en/p56-web-1.ts`; `.cin-reply-form` in `cinema.css` (logical properties).
- **SEC-AUD-056 (D-586)**: `ChannelPermissions` in `settings/server/sections/Channels.tsx` takes `needsManageRoles`
  from the channel-level `manageRoles` (new field on `ChannelPermissions`, computed in `liveMap.ts` by the shared
  permission engine, overwrites applied), falling back to the guild bit when the engine has not loaded. This disables
  the sync switch with the existing "need the Manage Roles permission" notice. Note it gates the whole Permissions tab
  (add role/member, overwrite editing), not only the switch, because they share the flag and the same hint.
- Sweep (`e2e/tests/page-sweep.spec.ts`): new `replyText` row option; rows `cinema-title-comments-reply` (opens the
  form, shots `*-form.png`, posts, shots the result) and `settings-server-channel-permissions`.

Commits: SEC-AUD-056, SEC-AUD-055, sweep rows, handoff (see `git log fix/p56-web`).

## Tests
- Vitest: `cinema.test.tsx` "replies ... (D-585)" (POST body with `parent_id` and spoiler; only one Reply button with a
  reply showing); `channels.test.tsx` two D-586 cases (guild-level allow + channel deny: switch disabled with hint;
  channel allow without guild bit: enabled). The channel test mock now overrides per-channel permissions.
- Gates: web tsc clean, prettier, css-tokens, i18n-audit, playwright `--list` loads (950 tests). Wider vitest run of
  cinema, settings/server, lounge, i18n: see the result below.

## Visual check
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p56-web`: `cinema-title-comments`, `cinema-title-comments-reply`
(+ `-form` variants), `settings-server-channel-permissions`, desktop and phone, light and dark passes, all looked at.
The Reply form sits under the comment with the rule on the reading side; the posted replies appear indented. Cinema
keeps its own dark editorial skin in both passes (same as before). The sweep Owner always holds Manage Roles, so the
channel-deny (disabled) state is covered by Vitest only, not shot.

## Not done / notes
- Nothing deferred. The sweep posts one reply per capture, so repeat runs show several identical replies.

## Owner questions
- Should the whole Permissions tab (not only the sync switch) follow the channel-level Manage Roles? Built that way.

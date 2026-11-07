# Wave 54 weba builder: handoff (SEC-AUD-036, SEC-AUD-037)

Branch `fix/p54-weba` (based on main after wave 53, `f581b538`). Nothing pushed. No Rust, no migrations.

## Done
- **SEC-AUD-036** (`bd925ed4`, audit MED theme 1, silent failures). One pattern: a failed action tells the member
  with the existing danger toast or an inline `lng-failed`/`Notice` line, and the screen keeps its true state.
  - `lounge/actionError.ts` (`reportActionError`, `failWith`) and `lounge/shell/ActionErrorToasts.tsx`, a
    render-nothing listener mounted next to `DmToasts` in `LoungeShell`, so plain functions (the live action
    object, menu builders, stores) can raise the existing `useToast` toast. No new visible component.
  - Message actions (`live.tsx`): delete, pin/unpin, vote, end poll, edit, reaction, forward. Store rollbacks
    (`store/store.ts`) now undo only what failed: a failed delete re-inserts the one message into the current
    window (it used to restore the whole earlier window, dropping messages that arrived meanwhile); pin, vote
    and reaction restore only their own field; a failed reaction now rejects instead of being swallowed.
  - History: a failed first load shows "Messages could not be loaded." with Try again (new optional
    `MessagesState.error`, `LoungeActions.reloadMessages`), and no longer the "Welcome to #channel" start row.
  - Menus: user menu Block / Ignore / Remove Friend / Invite to Server; Mute/Unmute for servers, DMs, the
    channel bell and threads; Close DM; member-list role add/remove and Message; Remind me; thread
    follow/leave; status change; soundboard favourites sync; server channel and member-list loads.
  - Inline errors: inbox friend accept/decline (row stays, toast), report form "also block" (sent screen says
    the block did not happen), message request answered then block failed (the request leaves the list, the
    error stays), scrapbook removal Retry/Dismiss and Clear shelf, reactor Remove (no longer "could not load"),
    temporary roles (errors were stored but never rendered), Extensions turned-on list and default-sticker
    switch loads, invite graph search ("did not work", not "no match"), channel thread defaults (the select
    rolls back on a refused save), the two "Copied" buttons (only after the clipboard really copied).
  - Failed is not empty: poll voters (loading and failure are no longer "Nobody"), thread starter preview
    (only a 404 means deleted), profile Watching/Reviews tabs (a 404 stays empty; other errors say so).
  - Notification dialog: a failed load shows an error with Try again (was a spinner forever); `load` returns
    whether it worked, and a read that left before a mute/level patch can no longer put the old value back.
  - Ignore and friend lists that fail at boot are retried every 30 s and on `online` (ignored people stayed
    visible for the whole session before).
  - Inline edit past the length limit says "N characters over the limit" (Enter silently did nothing).
- **SEC-AUD-037** (`29cfc603`, theme 2 remainder):
  - `settings/user/prefsSync.ts`: a failed first read (offline at page load) is retried on `online` and on
    the next change; a change the account copy has not taken is marked in localStorage
    (`sr.user.prefs.unsynced`), so the next load pushes it instead of letting the older account copy overwrite
    it. With nothing waiting, the account copy still wins as before.
  - Activity Privacy switches and the Library History pause switch stay disabled until the settings blob is
    known; a failed read shows an error with Try again (before, the defaults "sharing on" showed as if they
    were the member's choices). One shared blob read (`ensureSettingsBlob`) instead of one per hook.
  - Settings > Profile: a failed load now has Try again.
- **SEC-AUD-036 follow-up** (`95a06cef`): reaction tooltip and Reactors modal names come from
  the store (the route answers ids only); Extensions error spacing. Found in the visual check.
- i18n fragment `apps/web/src/i18n/messages/en/p54-weba-1.ts`. Visual-check spec `e2e/tests/p54-weba-shots.spec.ts`
  (local only, `SR_P54_WEBA=1`).

## Findings checked and not changed (false or already fine)
- `web-lounge-1` "swallowed errors in apps/*": confirmed fine (each shows a visible state), as the auditor said.
- `FullProfile` reviews error shown as "no reviews" was partly intended: the reviews route is not built yet
  (`lounge/profile/extras.ts` header), so a 404 must stay the empty state. Only non-404 errors changed.
- Profile settings "failed load then Save erases": not reachable; `dirty` is false unless the load succeeded.
  Only the missing retry was real (fixed).
- `FriendShelfShare`: a failed share read never sets the mount, so the switches do not show (no erase path).

## Not done (left for a later round)
- `lounge/shell/userSettingsSync.ts` (rail layout, collapsed categories, last channel): same offline pattern as
  prefs; a first read that failed is not retried, so offline layout changes are replaced on the next load.
- Inline edit: on a failed save the toast says so, but the typed edit is gone (the editor closes before the
  result). Keeping it open needs `LoungeActions.editMessage` to return a promise.
- `CreateDmDialog` orphan group on a partial failure, `DirectoryDialog` / scrapbook `Gallery` stale-filter
  races, `VoiceVideo` "Hide cameras" reverted by a later Save, `platformStaff` caching a failed check,
  `ViewAsRole` endless spinner on a malformed role, `keywords.ts` and sticker suggestion loads (silent),
  `ChannelModules` toggle overwriting unsaved values: correctness or race findings outside the two themes.
- Destructive-without-confirm findings (End Poll, Block, Remove Friend, stage End, ...): not silent failures;
  untouched.

## Visual check
Shots in `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p54-weba/` (45 PNGs, looked at): failed history
load, reaction-failed toast, poll voters failed, status-change toast (desktop only: a phone has the status menu
under You), notification dialog failed load, Settings > Profiles failed load, Settings > Activity Privacy locked,
Server Settings > Extensions failed list; Editorial, Lounge Light, Lounge Dark; desktop 1280x800 and phone
375x812. Spec: `e2e/tests/p54-weba-shots.spec.ts` (`SR_P54_WEBA=1`; it fails the requests with `page.route`).
Stack: p54-bin binaries, DB `sr_p54_weba_ui` (dropped), ports 8253/5253 (stopped).

Fixed because of what the shots showed:
- The reaction tooltip read "{who} reacted with 👍": the reactions route answers ids only (`UserListEntry { id }`),
  so the name was undefined and `t()` fell back to the raw template. Names now come from the users the store knows
  (`reactorName`, also used by the Reactors modal, whose rows were blank for the same reason). Before: the first
  run's editorial reaction shot (raw `{who}`); after: `toast-reaction-failed-desktop-lounge-light.png`.
- The Extensions load error touched the "Server look" heading; it now has the settings status spacing.

Seen, not mine: on a phone the poll card in a message row runs to the right edge (pre-existing layout).

## Tests run
vitest (web) over lounge, store, settings, moderation, admin, library, voice, i18n: 259 files, 2116 tests, all pass. New:
`lounge/__tests__/p54SilentFailures.test.tsx` (13), `settings/user/__tests__/p54PrefsSync.test.tsx` (4); both fail on
the code before the fix. `p11ShellProfile` now mocks the unbuilt reviews route as 404 (what the server's fallback
answers). tsc, eslint and prettier on changed files, `node scripts/i18n-audit.mjs` (0),
`node scripts/check-css-tokens.mjs` clean; `playwright test --list --project chromium` loads 932 tests in 74 files.
No public-page code touched, so no build or entrance/bundle check was needed.

## Owner questions
- Toasts for menu actions: a failed Block/Mute/Status from a menu shows a short danger toast (5 s). Discord shows
  a similar toast. Say if some (soundboard favourites sync, server/member-list loads) should stay quiet.
- The "unsynced preferences" mark is per browser, not per account (like the browser copy of the preferences
  itself). If two accounts share a browser and one leaves an offline change waiting, the next account to sign in
  there would receive it. Fine as is, or should the mark carry the account id?

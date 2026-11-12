# Handoff: fix/p61-web (SEC-AUD-134..137, wave 61)

Worktree `C:/Users/braxt/Desktop/SIDEROOM_wt/p61-web`, branch `fix/p61-web` (base `integration/phase60` at `5e5108fd`).
No Rust changed, no migration, `openapi.json` untouched. Not pushed.

## Done (commits on the branch)

| ID | what | files |
|---|---|---|
| SEC-AUD-134 | Lounge layout sync: the first settings read is retried on `online` and on the next local change (one read in flight, one try per event); synced keys changed on this device are marked in `localStorage` `sr.lounge.layout.unsynced` (`{user, keys}`), so when a read succeeds (this page or after a reload) those keys are pushed from the device and the others adopt the account copy; the mark is cleared after the write. | `lounge/shell/userSettingsSync.ts` |
| SEC-AUD-135 | `LoungeActions.editMessage` returns `Promise<boolean>` (live and mock). The inline editor is disabled while saving, closes on success, and on failure stays open with the text and an error line ("Your edit was not saved. Try again."); the toast stays; Escape still cancels; a late success does not close an editor that is no longer the same one. | `lounge/data.ts`, `live.tsx`, `mockData.tsx`, `MessageList.tsx`, `MessageItem.tsx`, `msg/InlineEdit.tsx` |
| SEC-AUD-136 | Verified: Notes keeps a non-extractable `CryptoKey` (usable by this origin without the passphrase) plus the wrapped bundle in IndexedDB `sr-tools-notes` / `keys` (`key:<userId>`, `bundle:<userId>`); only `Clear-Site-Data` removed it. `purgeOnSignOut()` now deletes the whole `sr-tools-notes` database (every account's key on this browser; see Build and budget for why not per account), through a tiny `pwa/notesKeys.ts` that does not import the Notes store (gives up after 2 s). `settings/user/api.ts` is unchanged. An event (`sr:notes-purge`) makes an open Notes store lock itself (the listener lives in `notesStore.ts`, loaded only with Notes). `logout`/`logoutAll` pass the current account id. | `pwa/notesKeys.ts`, `pwa/signOutPurge.ts`, `tools/notes/notesStore.ts` |
| SEC-AUD-137a | The cache was in `lounge/shell/platformStaff.ts` (not `useStaffRevoked.ts`, which is fine). Only 401/403 now means "not staff" and is kept; any other failure is not remembered and the next use asks again (3 s cooldown against hammering; the check runs on mount, not on every render). | `lounge/shell/platformStaff.ts` |
| SEC-AUD-137b | Real: when the engine refused a role/overwrite, `viewServerAs` returned `undefined` and the dialog showed "Working it out" for ever (also after a load error, with no retry). Now an error notice and Try again. | `settings/server/sections/ViewAsRole.tsx` |
| SEC-AUD-137c | The toggle patch already carried only `{enabled}`; the real bug was `apply()` resetting the typed form values from the answer. The switch now leaves the form values alone. | `settings/server/sections/ChannelModules.tsx` |
| SEC-AUD-137d | Real in the add-to-DM path (1:1 DM turns into a group, a later add fails): the failing add is collected, the rest continue, the group that exists is opened, and a dialog "Some people were not added" names them. All failed: the old plain error. (Creating a new group DM is one POST, so it has no partial state.) | `lounge/shell/CreateDmDialog.tsx` |

i18n: `apps/web/src/i18n/messages/en/p61-web-1.ts`. Visual spec: `e2e/tests/p61-web-shots.spec.ts` (local only, `SR_P61_WEB=1`).

## Tests (new or changed) and which I watched fail without the change

| test | file | failed without the change |
|---|---|---|
| a_failed_first_read_is_retried_on_online, a_change_made_before_the_first_read_is_pushed_not_overwritten, the_mark_survives_a_reload_and_is_cleared_after_the_write; with_nothing_waiting_the_account_copy_still_wins | `lounge/shell/__tests__/p61LayoutSync.test.ts` | the first three yes; the fourth is a regression guard (passes before and after) |
| a_failed_edit_keeps_the_editor_open_with_the_text, the_editor_is_disabled_while_saving; a_saved_edit_closes_the_editor | `lounge/__tests__/p61InlineEdit.test.tsx` | the first two yes; the third is a guard |
| sign_out_removes_the_notes_key (also checks the open-store lock event); a_blocked_notes_store_does_not_stop_the_purge | `pwa/signOutPurge.test.ts` | the first yes; the blocked test is a guard (the contract already held) |
| a_failed_check_is_asked_again_on_the_next_use; a_403_is_the_answer_not_staff_and_is_kept | `lounge/__tests__/p61PlatformStaff.test.tsx` | first yes; second is a guard |
| a_malformed_role_shows_an_error_with_try_again_not_a_spinner | `settings/server/__tests__/p61SmallFixes.test.tsx` | yes |
| a_toggle_keeps_values_typed_but_not_saved | same | yes |
| SEC-AUD-137d (group opens and names who was not added) | `lounge/__tests__/p11ShellDm.test.tsx` (added) | yes |

Also run and green: `lounge/__tests__/userSettingsSync.test.ts`, `p54SilentFailures.test.tsx`, `p11ShellDm.test.tsx`, `settings/server/__tests__/p18channelModules.test.tsx`. Web `tsc --noEmit`, eslint and prettier on the changed files, `node scripts/i18n-audit.mjs` (TOTAL 0) are clean.

## Visual check

Stack on 8392/5392, DB `sr_p61_web_ui` (dropped), spec `e2e/tests/p61-web-shots.spec.ts` (`SR_P61_WEB=1`, passes). Shots: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p61-web` (42 PNGs: `edit-failed`, `edit-saved`, `viewas-failed`, `viewas-retried`, `adddm-picked`, `adddm-partial`, `adddm-group-open`, each at desktop 1280x800 and phone 375x812 in lounge-editorial, lounge-light, lounge-dark). I looked at: all six `edit-failed`, `edit-saved` (phone light), `viewas-failed` (desktop light and editorial, phone light and dark), `viewas-retried` (phone editorial), `adddm-picked` (desktop light), `adddm-partial` (phone light, desktop dark and editorial), `adddm-group-open` (phone dark). Flows clicked by the spec: failed PATCH keeps the editor open with the text and the error line, then a successful PATCH closes it and shows the edited message; the View As Role dialog shows the error and, after Try again (roles and channels reread), the channel list; the add-to-DM flow opens the new group and the dialog names the person who was not added. Nothing wrong seen (the toast and the inline line both show on a failed edit, as the task asks; the toast overlaps the phone tab bar as every toast does).

## Build and budget

SEC-AUD-136 touches `signOutPurge`, which is in the first-paint chunk. On the base (`5e5108fd`) that chunk is 255,862 bytes gzip against the 256,000 limit (138 bytes of room). A first version of the Notes purge (open the DB, delete this account's two records, 2 s timeout, ~1.2 KB) failed the budget, so the helper is now `indexedDB.deleteDatabase('sr-tools-notes')` plus the lock event: the chunk is **255,967 bytes** (33 bytes of room), `check-bundle-budget.mjs` passes, `check-entrance-isolation.mjs` passes. Consequence: sign-out removes the Notes key database of **every** account on this browser, not only the one signing out (the other account re-enters its passphrase or recovery file next time). Any other builder adding to the first-paint chunk will break the budget at integration; the headroom is gone.

## SEC-AUD-136 note

The 401 exit path (`app/main.tsx`) cannot know the account, and a per-account delete cost too many bytes; so both paths delete the whole key database.


## Proposed inventory rows (after SEC-AUD-128)

- **SEC-AUD-134**: Lounge layout sync survives a failed first read: the read is retried (online, next change) and layout keys changed on this device are marked in localStorage so the next successful read pushes them instead of adopting the older account copy. Test: `p61LayoutSync.test.ts`.
- **SEC-AUD-135**: A failed inline edit keeps the editor open with the typed text and an inline error (Discord parity); the editor is disabled while saving. Test: `p61InlineEdit.test.tsx`, shots in `visual-sweep-shots/p61-web`.
- **SEC-AUD-136**: Sign-out removes the Notes key (non-extractable CryptoKey + wrapped bundle) held in IndexedDB and locks an open Notes store, without relying on `Clear-Site-Data`. Test: `signOutPurge.test.ts`. Note: the first-paint chunk has 33 bytes of budget left.
- **SEC-AUD-137**: Four wave 54 correctness leftovers: failed staff check not cached; View As Role error state with Try again; module switch keeps unsaved values; partial add to a DM opens the group and names who was not added. Tests: `p61PlatformStaff`, `p61SmallFixes`, `p11ShellDm`.

## Proposed threat-model rows

- Notes: a non-extractable key left in IndexedDB on a shared device is usable by anyone with that browser profile after sign-out; mitigation: the whole key database is deleted on sign-out (client side), `Clear-Site-Data` as the second layer. Residual: a tab killed before the purge runs, or a profile copied off the machine.
- Layout sync: an offline change can no longer be silently reverted by an older account copy (integrity of the member's own preferences; no cross-account effect, the mark names the account).

## Not done / decisions made

- Nothing dropped. (a) and (c) were real but in a different place than the plan named (above). (b) also got a retry that rereads roles and channels together, because the role list is handed in by the Roles page and may be the malformed one.
- Owner questions: none blocking. Defaults I chose: the 3 s retry cooldown for the staff check; sign-out deletes the Notes key database of every account on the browser (budget, above); on a failed partial add the dialog is a short notice, not an inline list.

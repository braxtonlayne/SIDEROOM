# Wave 53 web builder: handoff (SEC-AUD-008, 013, 014, 015)

Branch `fix/p53-web` (based on main `491aad85`). Nothing pushed.

## Done
- **SEC-AUD-008** (`d4ba760d`): `prepareGame` (`apps/web/src/emu/play.ts`) now returns `saveReadFailed` when the save read throws (`getSave` already answers `null` for "no save yet", 404, and throws for anything else). `useEmuGame` then starts the session with `saveMount = null` (no autosave, no unload save, no state save) and sets the new save status `readFailed`; `EmuPage` shows `games.saves.readFailed` and keeps the state buttons off. Test: `emu/__tests__/play.test.ts`.
- **SEC-AUD-013** (`9cfc5d72`): `revokeStaff(accountId, reason)` and `revokeInvite(id, reason)` send `{ reason }`; both confirm dialogs are now `ReasonDialog`s (reason required, server error shown inside the dialog). `admin/__tests__/admin.test.tsx` asserts the request body and that the confirm button is disabled until a reason is typed.
- **SEC-AUD-014** (`ff7ae6dc`): one pattern: failed load (anything but 404) shows an error with "Try again" and no way to save; 404 stays an empty form; a failed save never says Saved.
  - `GuildProfileDialog`: form stays locked (Save disabled), error + Try again.
  - `NoteDialog` (UserDialogs): error + Try again instead of the form.
  - `NoteField` (profile pop-out): box locked on failed load; a failed save shows an error and forgets the "saved" value so the next blur retries.
  - `BirthdayCard`: error + Try again instead of the form (before, the error showed over an editable default form).
  - `NotesTool`/`notesStore`: `store.save()` now resolves `true/false`; the editor shows "Not saved. Your changes are still here." instead of "Saved" when it is false. (The store already set `problem`, which the page shows.)
  - Tests: `lounge/__tests__/p53FailedLoad.test.tsx` (8), `tools/__tests__/notes.test.ts` (save return values).
- **SEC-AUD-015** (`3c05fcd2`), `packages/embedded-app-sdk/src/data.ts`:
  - Versions from the wire must be safe integers >= 1, at most `MAX_VERSION` (2^40), and for live changes at most local version + `MAX_VERSION_JUMP` (1024); snapshot ops must not exceed the snapshot's own floor, which is itself bounded. `ver: 1e308` is dropped.
  - New `ActivityData.scopeKeys(prefix)`: a key `<prefix><memberId>:...` can only be written/overwritten/deleted by that member (the host may delete, for compaction). Enforced on receive (live and snapshot ops, using the op's author) and on local writes (`FORBIDDEN`). The whiteboard calls it for `op:` and `blob:` in `adapterFromSdk`.
  - `sync` flood: the host answers one member's `sync` at most once per `SYNC_COOLDOWN_MS` (1 s).
  - Tests: `embedded-app-sdk/src/data.hardening.test.ts`, whiteboard `sdk.test.ts` (forgery refused).
- i18n fragment `apps/web/src/i18n/messages/en/p53-web-1.ts`. Visual-check spec `e2e/tests/p53-web-shots.spec.ts` (local only, `SR_P53_WEB=1`).

## Not done / caveats
- Emulator save-status shot: the emulator cores are not installed (no downloads allowed) so the page never reaches the running state; the `readFailed` line is covered by the `prepareGame` unit test only.
- No UI test of `NotesTool` itself for the "Not saved" label (needs key setup); the store contract is tested.
- See the bottom of this file for the visual check result.

## Owner questions
- `getSave` treats 403 like 404 ("no save yet"). Some S3 buckets without list permission answer 403 for a missing key, so I left it. If a bucket answers 403 for a real save the autosave could still overwrite it; say if 403 should count as "could not read" (then first-time saves on such buckets would run with saving off).
- Whiteboard keys now refuse a write under another member's id even by the host (the host may only delete). Fine with the current board; flag if a future pack needs host-written per-member keys.

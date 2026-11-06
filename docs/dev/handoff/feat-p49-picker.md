# Wave 49 handoff: picker (ACT-SDK-001, `pickShelfObject`, D-466)

Builder "picker". Branch `feat/p49-picker`, based on main `88b87885`. Not pushed, no PR.

## Done

- The member-only `pickShelfObject { accept?, max_size? } -> { shelf_id, object_hash, name, size, content_type } | null` bridge command, as D-466 decided.
  - SDK (`packages/embedded-app-sdk/src/protocol.ts`, `sdk.ts`): the command, `ShelfObjectPick`, `PickShelfObjectArgs`, `isPickShelfObjectArgs`, `isShelfObjectPick`, `sdk.commands.pickShelfObject(args = {})`. Not on the guest allow-list (`GUEST_COMMANDS` untouched; the guest test walks every command and now names this one).
  - Host (`apps/web/src/activities/host.ts`): `byos_read` required, arguments validated, one picker at a time (second call `FAILED`), `dispose()` aborts an open picker (the game gets `null`), the pick is checked again against `accept` and `max_size`, the host copies exactly the five fields, and `shelfAllowed` records the member's yes so the following `shelfObjectBytes`/`shelfObjectUrl` on that object shows no second dialog. Cancel returns `null`.
  - The dialog (`apps/web/src/activities/ShelfPicker.tsx`, lazy chunk loaded from `ActivityFrame.tsx`, never inside the frame): a `Modal` titled "Choose a file for {name}", a shelf `Select` (the shelves `GET /shelves/visible` names, via the Study's `visibleShelves`), a search box, the shelf's catalog (`GET /shelves/{id}/objects`, up to 15 pages) filtered by the game's `accept`/`max_size`, and a note of how many files were hidden. Pure filter rules are in `apps/web/src/activities/shelfPick.ts`. Strings are in `apps/web/src/i18n/messages/en/p49-picker-1.ts`; CSS (`.act-pick*`) is in `activities.css`.
  - Sketch & Slander (`packages/starter-packs/activities/sketch/client`): "Choose a file..." before the paste field when `platform.canPickPacks` (the host has `pickShelfObject`; never for a guest, never in the dev stand-in); it asks for `.json` and `.txt` up to the 64 KB pack limit, then loads the pick through the same path as a pasted reference (no second dialog). The paste field stays. The pick's file name is only a fallback pack name (a pack's own `name` wins).
- Docs: inventory row ACT-SDK-001 under a new "Wave 49" heading, decision D-488 (the details D-466 left open), threat-model row T51, `docs/specs/developer-platform.md`, the Sketch README.

## Tests

- SDK: `packages/embedded-app-sdk` vitest 41/41 (command shape, argument and pick checks).
- Host: `apps/web/src/activities/__tests__/pickShelfObject.test.ts` (capability refused, malformed args, cancel, accept and size re-check, five fields only, pick then `shelfObjectBytes`/`shelfObjectUrl` with no second prompt and another object still asks, second picker refused, frame closing cancels, filter helpers); `shelfPicker.test.tsx` (dialog: filtering, pick, cancel, search, no shelves, unreadable shelf); `guest.test.ts` (guest refused). `vitest --maxWorkers=2` on `src/activities` and `src/public/guest-play`: all pass.
- Sketch client: `packages/starter-packs/activities/sketch` vitest 31/31 (the platform's `pickPack`, older host, guest sends no pick command, the vendored SDK carries the command, the guest path names no member command).
- `e2e/tests/sketch-gate.spec.ts` extended (still the same single test): the first pack is picked in the app's dialog (the picker lists only the `.json` files under the cap and says 2 are hidden; no "Allow" dialog appears; one mint), the second pack is pasted (asks, as before); the gate asserts two mints and two member-page bucket fetches, none from the frame. Ran on my stack with the voice SFU and the guest relay: passes (dark and light; one dark run failed earlier at the lobby's player count of 3 instead of 2, before any picker code, and passed on rerun: a pre-existing flake in that spec).
- Gates: web `tsc --noEmit`, eslint on `src/activities` and the changed tests, prettier `--check` on all changed files, `check-css-tokens` (69 clean), `i18n-audit` (0), SDK `tsc`, e2e `tsc`, `playwright test --list --project chromium` (903 tests in 67 files), `pnpm --filter @sideroom/web build` then `check-entrance-isolation` (clean) and `check-bundle-budget` (all ok).

## Visual check

Screenshots: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p49-picker/` (`dark-*` and `light-*`: `desktop-picker`, `phone-picker` (375 x 812), `desktop-panel-choose`, `phone-panel-choose` (the lobby's pack panel with "Choose a file..."), `desktop-panel-picked` (the pack loaded after the pick), plus the gate's existing lobby, draw, reveal and guest shots). I looked at the picker in Lounge dark and Lounge Light on desktop and phone, the lobby panel with the button, and the loaded pack; the flow was clicked through by the gate spec (open, filter, pick, no second dialog, pack loads, round plays).
Not shot: Editorial. An Activity frame, and so the picker, exists only in the Lounge (a voice or text channel), so there is no Editorial surface for it.

## Not done / notes

- A catalog with more than 3000 objects in one shelf is read only up to 15 pages (200 each); the search narrows what is listed but cannot reach past that. Fine for pack-sized use; a "load more" would be a follow-up.
- The test stack has no real bucket: the picker's two routes and the mint/bucket routes are played by Playwright routes in the gate spec (as the spec already did for the mint and bucket).
- No Rust touched; no cargo run. Migrations: none.

## Owner questions (answered by Fable: D-495 flat catalog, D-496 no expiry timer, D-497 `<input accept>` grammar; no behavior change)

1. The picker lists a flat, searchable catalog (the catalog route has no folders) of one shelf at a time. Discord-like default; would the owner rather have folders (the Study's directory listing, which has no content hashes and would need one extra hash lookup per file)?
2. The pick lasts for the frame's life, like a consent dialog's yes (D-466 says "for that one object"; the host keeps it in `shelfAllowed`). Should it also expire after a few minutes, or when the member reloads the page (it does today)?
3. `accept` entries are file extensions or media types (`.json`, `json`, `text/plain`, `image/*`); media types are guessed from the extension. Is that the right grammar for SDK docs, or should only extensions exist?

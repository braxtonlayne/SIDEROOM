# feat/arcade-redesign: The Arcade redesign (ARC-UI-010)

Builder "arcade". Branch `feat/arcade-redesign` from main `d7cf7380`. Not pushed.

The owner said the Arcade looked broken: a bare column with a title, three chips, one plain Emulator card and an
always-open "List a cartridge" form. This branch changes how the page looks. Every API call and behaviour stays the
same.

## Done

| Commit | What |
|---|---|
| `53632bf5` | `GameArt.tsx`: in-house inline-SVG cover art (Land.io, Arena 3D, Hold'em, Whiteboard, Tabletop, Emulator, plus generic "featured", "cartridge" and "workshop" art coloured by a hue taken from the title). `builtins.ts`: the built-in games catalogue. `GameCard`: art, a status ribbon (Playable now / In development), a meta line, a footer, `focusable` and a `feature` size. |
| `8d2e58e3` | The page. A hero with an eyebrow, title, one-line pitch, a "How to start a game" pair (the + menu, the call bar) and jump links. **Playable now** shows the Emulator as a wide featured card plus the five built-ins. It draws straight away, with no data needed. **Featured** shows the Operator's picks and has a designed empty state. **Server cartridges** has an empty state that explains what cartridges are. "List a cartridge" is now a button that opens a `Modal` dialog. The dialog closes when listing succeeds and stays open with the reason when it fails. The server picker shows only with more than one server. **Coming soon** has workshop art, a Notify me toggle (same behaviour and `aria-pressed`), the repository link and an empty state. A loading skeleton and an error state with **Try again** (re-runs `front()`). New strings are in `i18n/messages/en/p47-arcade-1.ts`. CSS is rewritten with tokens and logical properties only. |
| `74582b8f` | The Showroom's sample Arcade uses the same hero and cards: the built-ins plus the fixtures, split into Playable now and Preview / in development. The `arcade-room.spec.ts` selectors are scoped to the Featured region, because "Land.io" is now also a built-in card. |

Sketch & Slander: add one `BUILTIN_GAMES` entry (`status: 'preview'` puts it under Coming soon, `'live'` under Playable
now) and the `arcade.builtin.<id>.name` and `.pitch` strings. Until someone paints it, an unknown `art` falls back to
the generic painting.

## Visual check (AGENTS.md section 4)

- Before: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/arcade-before/` (`arcade`, `showroom-arcade`,
  `admin-arcade`, both viewports and both themes). The `admin-arcade` before-shots are blank (that capture failed on
  the fresh world). The after-shots render.
- After: `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/arcade-after/` (same routes; the sweep flags 0 issues
  on all three), plus `arcade-after/flows/`: the populated Front Page (Featured picks with tips and the Bitcoin QR open,
  Coming soon with tier chips) and the List a cartridge dialog, desktop and phone. The flows were shot by a scratch
  probe that was deleted afterwards.
- Fixed while iterating: raw keys (Vite needed a restart to pick up the new fragment); the grid went from 4 to 3
  columns (no orphan card); the empty states are more compact; the Tabletop fog is lighter; the tip buttons no longer
  stretch; a Featured pick that can't be played in the browser (desktop only) gets no "Playable now" ribbon; the
  header "List a cartridge" button no longer jumps into the empty state after loading.
- Clicked through: open and cancel the dialog, list a cartridge (the dialog closes and the card appears), unlist it,
  Notify me, Bitcoin QR, Open the emulator.

## Tests and gates

- vitest `src/arcade src/showroom src/emu`: 156 passed. New tests cover: every built-in on Playable now with art,
  tabindex and readable chips; the Emulator card switched on and off; a built-in in preview goes to Coming soon; the
  generic art fallback; the empty Coming soon; the empty-server state opening and cancelling the dialog; the picker
  only with more than one server; a failed listing keeps the dialog open with the reason; stable hues; unique SVG
  ids. Existing tests were changed only where the structure changed: the form now opens from the button, and
  FeaturedSection no longer takes `emulatorOn`. No assertion was dropped.
- Web tsc, e2e tsc, eslint and prettier on the changed files, `check-css-tokens` (69 clean), `i18n-audit` (TOTAL 0),
  web build, `check-entrance-isolation` (clean), `check-bundle-budget` (all ok). The new code lands only in the lazy
  `ArcadePage` chunk and `arcade` CSS, so first-paint growth is zero. `playwright test --list`: 902 tests in 66 files.
- `e2e/tests/arcade-room.spec.ts` against my stack (8203/5203, `sr_arcade_ui`): passed.

## Not done

- No inventory row was added for ARC-UI-010. The sketch builder is adding the Wave 47 section to
  `docs/inventory/21-feature-phase.md`, so the integrator should add the row there and avoid a merge conflict.
- Admin > Arcade only shares the stylesheet. Its look is unchanged; the old section rule was kept for pages outside
  the Arcade.

## Owner questions (defaults applied)

1. **Art colours.** The cover paintings use fixed hex palettes inside the SVGs: a box cover, not a theme surface.
   Everything around them uses `--ui-*` tokens. `check-css-tokens` doesn't target `arcade/`, but run with
   `arcade/GameArt.tsx` as an explicit argument it would flag them. OK as art, or should the art be tinted from
   theme tokens?
2. **Server picker.** It shows when the member is in more than one server, not "more than one server with
   cartridges". That rule would mean fetching every server's list up front, which is a new request pattern.
3. **Built-in cards have no Play button**, because games start from a channel and need a channel id. The cards and
   the hero say where to start. Should a later wave add a "Start in…" channel chooser?
4. **Hold'em is shown as "2 to 9 players"** (the engine's `maxSeats`; the manifest declares no count). Whiteboard
   and Tabletop show no player count.
5. **Featured stays a section with an empty state** when the Operator has picked nothing (an existing test requires
   the "Nothing is featured yet" copy). Should it be hidden while it's empty instead?
6. Hero copy ("Games night, any night", the pitch) is new. The old intro about who owns the code, art and sound
   (`arcade.page.intro`) is no longer shown on the page.

## Stack

sr-server 8203 and Vite 5203 (started by me) are stopped, and the `sr_arcade_ui` database is dropped. The binaries
copy is in `C:/Users/braxt/Desktop/SIDEROOM_wt/arcade-bin`.

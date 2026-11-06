# Wave 49: ARC-ART-001, the Arcade's cover scenes (branch `feat/p49-art`)

This narrows the gap between the covers and the owner's painted concept (`design/arcade-concept.webp`); it does not
close it, and that is fine for v1 (D-481). The covers are still flat-shaded inline SVG, now lit as scenes.

**For the owner:** open `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p49-art/contact-sheet.html`. It shows
before (main) and after side by side: every cover at the hero, row and phone crops (the generic kinds at hues 0, 60,
120, 200, 280 and 330), the populated Arcade at 1280, 375 and 320 px, and the sweep of `arcade` and `showroom-arcade`
in Editorial, Lounge Light and Lounge Dark at desktop and phone.

## Done

| Commit | What |
|---|---|
| `1b1c7dee` | `apps/web/src/arcade/GameArt.tsx`. **(a) The three generic kinds are scenes**, each coloured by `hueOf(title)`: `featured` is a stage with a curtain in the game's hue (house curtain tied back, a scalloped valance, boards), one spotlight from the upper left and an arcade cabinet in its pool; `cartridge` is a cartridge standing on a wooden table under a desk lamp, its shell and label in the hue, a second cartridge lying in the foreground; `workshop` is a drafting table under a clamp lamp, the blueprint in the hue with ruled lines and a cabinet drawn in plan, a T-square, a set square, a pencil and a sheet pinned to the wall. **(b) Depth pass on the seven built-ins**: a darker band behind the subject, a lit side and a shadow side, soft cast shadows to the lower right, and a stronger `Frame` vignette (shared by all ten). Land.io's subject is now a raised home slab in the middle with the trail leading out of it; Arena 3D's crate and player moved from the edges into the middle, with the sun moved to the upper left so the light agrees with the lamp. Hold'em's cards and a chip stack, the whiteboard's ellipse and sticky note, the d20 and the handheld all sit in the subject box. **(c)** Every subject is inside x 120–200, y 20–100. Helpers: `Soft` (a soft ellipse used for every shadow and light pool), `Band`, `Beam`, `Folds`. No `filter` anywhere, no animation, fixed palettes, `aria-hidden` (D-470). |
| `c6375e90` | `arcade.test.tsx`: every kind renders `aria-hidden`, with its `data-art`, at most one `filter`, no SMIL animation, and every `url(#…)` resolves inside the same painting; a generic scene's colours are stable for a title, differ between titles, and use the title's hue. |
| `2dd0b5e3` | `scripts/bundle-budget.json`: a new `arcade` row (`routeChunk: "ArcadePage"`), measured 269.3 KB gzip, limit 310 (+15%). Before the art: 266.8. The `ArcadePage` chunk (where `GameArt` lives) grew from 14 135 to 16 641 bytes gzip, **+2.4 KB**, against the 10 KB allowance. |
| `06f08fc4` | **Fix (older bug, from the coordinator):** a Featured pick that is the same game as a built-in (D-477: by shelf id, else by name) painted the generic stage. `pickArt()` in `builtins.ts` now gives it that built-in's own cover in the hero slot and the Featured card (`FeaturedSection` takes `shelfIds` from `ArcadePage`); any other pick keeps the generic `featured` scene. Tested in `arcade.test.tsx` (by name, by shelf id, and an unmatched pick, in both the hero and the Featured row). `after/flows/` re-shot: Land.io's pick now shows the Land.io painting. |
| `297e419b`, this commit | D-490 in the decisions log, a note on ARC-ART-001 in the inventory, this handoff. |

## Fable's decisions (D-491 to D-494)

The owner questions below were decided by Fable (`SIDEROOM_wt/p49-fable-decisions.md`): keep the cabinet (D-491); keep
the phone banner and D-490 (D-492); fold hues 45 to 75 onto gold in `hueOf` (D-493, done, with a test; `after/art-sheet.png`
re-rendered, its 60 row is now "Lantern Keep", which hashes to 48 and paints at 37); ARC-ART-001 accepted and ticked
(D-494). The populated `after/flows/` shots predate D-493, so Lantern Keep still shows its olive cover there.

## Not done

- Admin > Arcade shows no covers, so there was nothing to shoot there.

## Visual check (AGENTS.md section 4)

All in `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p49-art/`:

- `before/`, `after/`: `arcade` and `showroom-arcade`, desktop 1280 and phone 375; `-dark` = Editorial (the default),
  `-light` = a Lounge Light member. `before-loungedark/`, `after-loungedark/`: the dark pass with `SR_SWEEP_DARK=lounge-dark`.
- `before/flows/`, `after/flows/`: the populated Arcade at 1280, 375 and 320 px: three Featured picks (Land.io taking
  Land.io's hero slot, the desktop-only Horizon, Lantern Keep), two server cartridges (a game and a mod) and two Coming
  soon entries. Seeded like round 4 (`feat-arcade-redesign.md`): a scratch Playwright probe (the `solo` fixture) that
  answers `GET /api/v1/arcade` with `arcade-room.spec.ts`'s front page plus a pick and an entry, and
  `GET /api/v1/guilds/*/arcade/cartridges` with two cartridges. The probe was not committed.
- `before/art-sheet.png`, `after/art-sheet.png` (and their HTML, `art-sheet-before.html`, `art-sheet-after.html`):
  every painting at the hero crop (420 x 270, with the hero's wash and a title), the desktop row slice (110 x 245),
  the phone thumbnail (112 x 144) and the whole viewBox with the subject box dashed. Rendered by a scratch vitest
  file with `renderToStaticMarkup` (one React tree, so gradient ids stay unique), not committed.
- `contact-sheet.html`: the before/after pairs of all of the above.

Looked at, every shot. The sweep flags 0 issues on both routes in every pass. Found and fixed while iterating:
Arena 3D's player and the whiteboard's sticky note were cut by the row slice (moved in); the cabinet was too dark to
read in its own spotlight (lighter front); the workshop read as a framed sheet on a floor (now a table on legs over a
floor, with a pinned sheet on the wall); on a phone a server cartridge's art is a ~3:1 banner that cut the
cartridge's top (the cartridge is now scaled about its foot to fit, D-490). Text over the hero art stays readable:
the hero wash covers the lower left, and the subjects sit to its right and above it. The only overlap is Arena 3D's
hero, where a three-line pitch lifts its title to the height of the crate's left edge; the title stays on the wash and
reads clearly.

## Tests and gates

- web `tsc --noEmit`: pass. eslint and prettier on the changed files: pass.
- vitest `src/arcade src/showroom src/emu` (`--maxWorkers=2`): 185 passed in 19 files.
- `check-css-tokens`: 69 files clean. `i18n-audit`: 0 (no new strings).
- web build: ok. `check-entrance-isolation`: clean. `check-bundle-budget`: all ok; `app` unchanged at 246.9 KB,
  `arcade` 269.3 of 310.
- `playwright test --list --project chromium`: 903 tests in 67 files.
- No Rust touched.

## Owner questions

1. **The `featured` scene's subject.** D-481 asked for "a stage with a drawn curtain and one spotlight pool"; I put an
   arcade cabinet in the pool so the cover says "a game" at the row crop, where the curtain alone reads as fabric.
   Keep the cabinet, or an empty stage?
2. **The phone list banner (D-490).** A server cartridge's art on a phone is a ~3:1 banner. I fitted the cartridge to
   it rather than change the card. If the owner would rather the phone list card use the same 7 x 9 rem thumbnail as
   the row cards, that is a layout change for its own task.
3. **Hue at 60 (yellow-olive).** A title that hashes near 60 gets an olive curtain and blueprint. It reads as a scene
   but is the least attractive hue. Leave it, or nudge hues 45–75 toward gold?

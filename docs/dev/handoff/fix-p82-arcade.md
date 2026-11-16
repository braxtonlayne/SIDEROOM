# Wave 82 handoff: arcade (SEC-AUD-340, D-844)

Worktree `C:/Users/braxt/Desktop/SIDEROOM_wt/p82-arcade`, branch `fix/p82-arcade`. Web only, no cargo, no Rust, no migration.

## Clock
T = 11:10 (2026-11-16). No new item from T+2h = 13:10; checkpoint T+2h20 = 13:30; finished T+2h45 = 13:55; hard limit T+3h = 14:10.
Actual: spec and CSS ready 11:09 (the clock ran slower than my work: first `date` 11:06), waited for the heavy lock 11:09 to 11:39 (tools, developers and library took it first), round 11:39 to 11:43 (stack up, base run, after run, shots opened), lock released 11:42, handoff committed about 11:45. Finished well inside T+2h45.

## Done
- `apps/web/src/arcade/arcade.css`: a block on `.arc-card[data-size='hero']` (placed above the "two house picks" rule) that states every colour token the card and what stands in it read, with Editorial's values, plus `color: var(--ui-text)` and `color-scheme: dark`. Comment names SEC-AUD-340 and D-844. No radius, no font (D-831). No box changed.
- `e2e/tests/p82-arcade-shots.spec.ts` (new, local only: `SR_P82_ARCADE=1`, `SR_P82_ARCADE_OUT`, `SR_P82_BASE=1` for the base run): three themes x two sizes, `data-theme` read back from the room's root, token probes inside a hero card and inside a row card, every card's box, sideways scroll, text colour and the 3x3 pixel grid behind title, description, meta line and kicker (second shot with the card body hidden) with contrast, a hash of each hero card's pixels, the Play button at rest, with the pointer and with keyboard focus, the click-through (press Play, Escape). The after run asserts: row card probes and every card box equal the base in all three themes; Editorial's hero probes, sample grid and pixel hashes equal the base's; light and dark hero probes equal the Editorial row; every text sample is at least 4.5:1.
- Commit: `SEC-AUD-340: an Arcade hero card is a surface kept dark (D-844)` (arcade.css and the spec; a later small edit to the spec is in the second commit).

## What I saw on the base (I ran it: arcade.css held back, `SR_P82_BASE=1`, the same spec, same stack)
- Light (`lounge-light`), 1280, the two hero cards: Land.io ("our flagship" kicker, title, description) and Arena 3D (title, description, meta line). The painting is washed by `--ui-bg` (white) into a pale veil, the title is dark text (`rgb(49,51,56)`).
- Contrast, base, worst of 9 samples per text:

| text | editorial | light | dark |
|---|---|---|---|
| Land.io kicker ("OUR FLAGSHIP", accent colour) | 6.4 (it is dark-on-dark gold) | **1.67** | **2.50** |
| Land.io title | 11.7 | 5.21 | 9.21 |
| Land.io description | 10.68 | 8.3 | 8.13 |
| Arena 3D title | 4.97 | 5.11 | **3.83** |
| Arena 3D description | 10.94 | 4.88 | 8.06 |
| Arena 3D meta line | 7.1 | 4.9 | 5.48 |

  So on the base in light the title and description are all at least 4.5:1 (the plan expected one under; the lowest is the description at 4.88), but the kicker is 1.67:1 (blue `#5865f2` over a pale veil), and in Lounge Dark (which the plan did not list as a fault) the kicker is 2.5 and Arena 3D's title 3.83 where it crosses the painting's lighter band. The washed painting is visible in `base-light-desktop-top.png` (a pale veil, the painting almost gone, a white "ORIGINAL" tag, a blue Play button). I built it anyway as the task says.

## Token list (every colour the card reads), value and where read
Read from `packages/ui/src/tokens/themes.ts`, `loungeEditorial` (lines 176 to 230: it spreads `loungeDark.tokens`, which spreads `loungeShared`; every value below is stated by Editorial itself, none comes through the spread), and probed under the shipped theme on the stack (the probe's computed colour equals each, see "After"):

| token | value | read by |
|---|---|---|
| `--ui-bg` | `#23221d` | the wash (`::after`), `.arc-tag` ground, the painting's ground |
| `--ui-bg-raised` | `#1c1b17` | the card ground |
| `--ui-bg-overlay` | `#1c1b17` | stated for completeness (tip popover) |
| `--ui-bg-hover` | `#2c2a24` | kit button (ghost/secondary hover), `.arc-cta` hover |
| `--ui-border` | `#3a382f` | card border, tag, chips, tips |
| `--ui-text` | `#efe6d2` | title, description, kind, tip pills, kit button text |
| `--ui-text-muted` | `#b3ac9a` | meta line, chips, tag |
| `--ui-text-faint` | `#a39d8b` | stated for completeness |
| `--ui-accent` | `#c2b08f` | kicker, Play button, hover border |
| `--ui-accent-hover` | `#d1c1a2` | Play button hover |
| `--ui-accent-ink` | `#1b1a15` | Play button text |
| `--ui-focus` | `#d9c9a4` | card and button focus ring |
| `--ui-danger`, `-ink`, `-text` | `#b4503f`, `#fff6f2`, `#d97a68` | start error, kit danger button |
| `--ui-success` | `#8c9a6a` | live chip dot (arcade.css) |
| `--ui-warning` | `#c2b08f` | "In development" dot |
| `--ui-info` | `#9aa8a0` | stated for completeness |
| `--ui-status-online/idle/dnd/offline` | `#34c25f`, `#d9a441`, `#cc5c47`, `#6f6c60` | status dots |
| `--text-link` | `#c2b08f` | links in running text |

All values are literals in Editorial, so no `var()` or `color-mix()` had to be flattened. The wash's `color-mix(... var(--ui-bg) ...)` is computed on the card's own `--ui-bg` (the pseudo-element inherits it), so it recomputes inside the block. The kit button reads `--ui-accent`, `-hover`, `-ink`, `--ui-bg-hover`, `--ui-border`, `--ui-text`, `--ui-text-muted`, `--ui-danger`, `-ink`, `--ui-focus` (read in `packages/ui/src/primitives/Button/Button.module.css`; read only). `--ui-radius`, `--ui-font-*` are not stated.

## After (I ran it, same grid)
- Shipped (`lounge-editorial`): every probed token (23 colours, the card's own colour, border colour and `color-scheme`) equals the base; the sample grids of all six text items equal the base's; the two hero cards' element-shot hashes equal the base's (`d5c7daf5...`, `e89d9bf5...`); every card's box equals the base's; the row card's probes equal the base's. Pixels did not change under the shipped theme.
- Light and dark: the hero probes equal the Editorial row exactly; the row card probes and every card box equal the base's.
- Contrast after, worst of 9 samples (the three themes are the same colours; sample positions differ a little in light and dark because the title's font follows the member's theme, D-831, so its box is slightly different and its hash differs from Editorial's):

| text | editorial | light | dark |
|---|---|---|---|
| Land.io kicker | 6.4 | 6.4 | 6.4 |
| Land.io title | 11.7 | 11.51 | 11.51 |
| Land.io description | 10.68 | 10.68 | 10.68 |
| Arena 3D title | 4.97 | 4.6 | 4.6 |
| Arena 3D description | 10.94 | 10.94 | 10.94 |
| Arena 3D meta line | 7.1 | 7.1 | 7.1 |

  No sample under 4.5 (`FAILURES(<4.5) none`). The lowest (4.6 to 4.97) is the Arena 3D title where its lower-right letters cross the painting's lit floor: the same in the shipped theme, unchanged by this change.
- Play button in a hero card, light: rest `rgb(194,176,143)` on `rgb(27,26,21)` (Editorial's gold, whole), pointer `rgb(209,193,162)` (accent hover), keyboard focus outline `rgb(217,201,164)` solid 2px (Editorial's focus); "ORIGINAL" tag reads. Row and list cards and the page keep the member's theme (blue Play buttons in light in the shots).
- Click-through (light, 1280 and 375): press Play: the "Start Land.io in..." dialog opens (1 dialog); Escape closes it (0). At 375 the pressed Play is a row card's (see findings).
- No page scrolls sideways: `scrollWidth == clientWidth` at 1280 and 375 in all three themes (1280/1280, 375/375).
- Vitest `arcade/__tests__/arcade.test.tsx` single file: 40 passed (it asserts no CSS, so it passes with and without the change). `node scripts/check-css-tokens.mjs`: 73 files clean. prettier clean, `node scripts/i18n-audit.mjs --max 0`: TOTAL 0, e2e `tsc --noEmit` clean.

## Findings (not fixed, for a later Arcade look pass)
1. **At 375 the hero cards are not shown at all**: `arcade.css` (phone block, `.arc-hero-cards { display: none; }`) hides both. The change therefore has no phone effect; the phone's top is the headline and then "Playable now" row cards (themed by the member, light/dark verified in the shots). The task's "phone" checks on the hero cards (contrast, pixels) are vacuous: the spec logs `hero cards shown: false` at 375 in all three themes. Left as it is (a design decision, not mine).
2. In the shipped theme the text on the hero cards covers the lower left of the painting, about a third of it, and the wash hides the bottom half; the subject (the Land.io red block, Arena 3D's capsule) stays clear on the right. No finding beyond that.
3. The Arena 3D title in all three themes crosses the lit floor of the painting at its lower-right (4.6:1 to 4.97:1), above the 4.5 limit but close.

## Docs, imports
- `git grep -n "arcade.css" -- apps/web/src`: imported by `arcade/ArcadePage.tsx`, `arcade/ArcadeShell.tsx`, `showroom/PreviewArcade.tsx` (lazy, `ArcadePage` is a route chunk, the Showroom is `lazy()`) and `admin/AdminApp.tsx` (the Admin chunk, lazy). None is first paint or a public page: no web build run. (The Admin's Arcade page does not render a hero card.)
- Docs: `docs/10-design-system-and-theming.md` line 23 lists what a surface kept dark is. Proposed addition for the commander (outside my files): after "(D-820, D-831, D-832)" add "; an Arcade hero card is one too (D-844)". No other doc states the hero card's colours.
- A stash was not used; none touched.

## Shots (folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p82-arcade`, 12 PNGs, I opened all 12)
- base: `base-light-desktop-top.png`, `base-editorial-desktop-top.png` (2).
- after (10): `after-{editorial,light,dark}-{desktop,phone}-top.png` (6), `after-light-desktop-play-pointer.png`, `after-light-desktop-play-focus.png` (2), `after-light-desktop-play-pressed.png`, `after-light-phone-play-pressed.png` (2).
- The textless shots behind the pixel samples are decoded in memory and not saved; the samples and contrasts are in `report-base.txt`, `report-after.txt` and `data-*.json` in the same folder.

## Proposed inventory row (SEC-AUD-340)
"SEC-AUD-340: an Arcade hero card is a surface kept dark (D-844). `.arc-card[data-size='hero']` states every colour token it reads with Editorial's values and `color-scheme: dark`; under a light or dark member theme the card's probes, wash and text are the shipped theme's. Measured: light kicker 1.67:1 and Lounge Dark's Arena 3D title 3.83:1 on the base, all text samples at least 4.6:1 after; the shipped theme's probes, sample grid and pixels equal the base. The phone hides the hero cards (not a fault of this row)." Closes: SEC-AUD-340.

## Changelog sentence
The Arcade's two big house-pick cards keep their dark painting, text and Play button in a light theme (and in Lounge Dark), where they had turned into a pale veil with an unreadable "Our flagship" line.

## Owner questions
1. At 375 the two hero cards are hidden (display none, `arcade.css`): should a phone show them (a compact one-card hero), or is hiding them the decision? Default kept: hidden.

## Not done / cleanup
- Nothing left undone in the task. No second lock round needed.
- Cleanup done: stack stopped by PID (server 15512 serving 8602 and 9302, Vite 14404), `sr_p82_arcade_ui` dropped, `p82-arcade-bin` deleted (`p80-base-bin` untouched), lock released. The scratch folder `C:/Users/braxt/Desktop/SIDEROOM_wt/p82-arcade-scratch` (base/new CSS copies, the start script, logs; outside the repo) can be deleted by the commander. `git status` clean apart from the files named here; no `data/` folder in the worktree.

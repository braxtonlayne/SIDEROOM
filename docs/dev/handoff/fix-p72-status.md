# Wave 72 handoff: status (SEC-AUD-276; D-756)

Branch `fix/p72-status`. Web only; no migration, no route, no Rust, no new string.

## Done (commits)
- `SEC-AUD-276: Editorial's online colour is a clear green; the phone glyph is a filled shape (D-756)`: `packages/ui/src/tokens/themes.ts`, `icons/paths.ts`, `primitives/Avatar/Avatar.module.css`, two vitest files.
- `SEC-AUD-276: status shots spec (local only)`: `e2e/tests/p72-status-shots.spec.ts` (`SR_P72_STATUS=1`).
- `SEC-AUD-276: docs state the status colours and the solid phone glyph (D-756)`: `docs/10-design-system-and-theming.md`, `docs/specs/lounge-chat.md`.
- This handoff.

## Confirmed on the base first
Base shots (`visual-sweep-shots/p72-status/base`, 39) vs after (`.../new`, 39), same spec, same stack (base-bin built from 8bc63455), packages/ui put back to the base commit for the base run. On the base, Editorial's online dot and glyph read as a grey-olive: the dot on the dark brown is hard to call a colour, the 13 px outline phone is a faint olive line (Lounge Light and Dark's `#23a55a` read plainly green beside it). So the fault was there; fixed.

## Colours
| | old | new | on `--ui-bg` / raised / lowest | hover row |
|---|---|---|---|---|
| Editorial online (`lounge-editorial`, also `sideroom` and `sideroom-broadcast`, same olive) | `#8c9a6a` (5.27 / 5.70 / 6.15) | `#34c25f` | 6.85 / 7.41 / 7.99 (Editorial); 8.12 / 7.29 (sideroom bg / raised) | 6.17 |

All at least 3:1 on every surface the contrast test lists. Idle `#c2b08f`, do-not-disturb `#b4503f`, offline `#6f6c60` are unchanged: in the zoomed shots all four are told apart (green, tan, red, grey). The Lounge themes' values are unchanged. Side note, not changed: Editorial's do-not-disturb is 2.84:1 on its hover row (`--ui-bg-hover` `#2c2a24`); the contrast test of wave 71 does not list that surface and it reads fine in `list-hover-desktop-lounge-editorial.png`.

## Glyph
The `smartphone` icon (`packages/ui/src/icons/paths.ts`, drawn by `Icon`, used only by `apps/web/src/lounge/phoneStatus.tsx` `statusProps`, which every Avatar that draws a status spreads; no second copy of the path exists) now has a third subpath, the screen. `.statusIcon > svg` in `Avatar.module.css` sets `fill: currentColor; fill-rule: evenodd; stroke: none`: a solid phone body in the status colour with the screen cut out (I judged the cut-out version at scale 3: it reads as a phone, a plain slab reads as a green pill). Same `Icon name`, size and props; the accessible name ("Online on a phone") is untouched and asserted; the `page-colour ring` rule is unchanged. Drawn as an outline elsewhere it would be a phone with a screen (nothing else uses it).

## Tests
- `packages/ui/src/theme/tokens-contrast.test.ts`: new "status colours reach 3:1 on every surface" (Editorial family: `lounge-editorial`, `sideroom*`) and "online colour is a clear, saturated green" (hue 100-165, HSL saturation >= 0.5, all themes). Watched fail: on the base values the saturation test fails for `lounge-editorial`, `sideroom`, `sideroom-broadcast` (olive saturation ~0.2); with a pale green `#9bc9a6` tried in place of the new value, the same three fail. The Lounge-themes all-surface check is not asserted: Lounge Light's `#23a55a` is 2.87:1 on `#f2f3f5`, by that palette.
- `packages/ui/src/primitives/display.test.tsx`: the glyph under its given name, `data-icon="smartphone"`, three subpaths. 20/20 pass. `tokens-contrast` 60/60 pass. `packages/ui` `tsc --noEmit` clean; spec passes prettier; `i18n-audit --max 0` TOTAL 0; `check-doc-links` ok.
- No test was run against the base for the icon test (it asserts the new path).

## Shots
`visual-sweep-shots/p72-status/new` (39) and `/base` (39): member list desktop and phone (real swipe), hovered row, profile card, user panel, DM list desktop and phone, in Editorial, Lounge Light, Lounge Dark; scale-3 close-ups of every status row and the phone glyph. **I opened all 39 of `new`; of `base` I opened three** (list-desktop-editorial, zoom-phonerq-editorial, zoom-glyph-phone-light) as proof of the fault. Not done: a dot on an avatar with a decoration and on the smallest avatar size (no decoration was seeded; the dot geometry is unchanged, only colour and the glyph fill moved).

## Side effect to know
`--ui-status-online` is also the app's general "green" (voice connected, speaking ring, the rail's add-server, explore and music icons, TV reception, settings meter, the "success" message button). Editorial now shows those in the same clear green as the Lounge themes (visible in `list-desktop-lounge-editorial.png`: the rail icons are green, as they are in Light and Dark). No Lounge rule overrides the status colour; I changed no Lounge CSS.

## Bundle
`pnpm --filter @sideroom/web build` ok. `check-bundle-budget`: first-paint `app/app-Brow7TzO.js` 215,870 bytes gzip, room 40,130 to the limit (needs 8 KB). Before: no base build of mine; wave 71 ops reported 215,813 (room 40,187), so at most +57 bytes (the icon subpath and the CSS line). `check-entrance-isolation` clean; `check-staff-delivery` ok (all rc 0). Nothing in the changed files names a section or theme.

## Proposed rows and sentences
- Inventory SEC-AUD-276: "Editorial's online status colour is a clear green (`#34c25f`, at least 3:1 on its surfaces, a saturation floor in the contrast test) and the phone status glyph is a solid shape with the screen cut out; Lounge themes unchanged (D-756). Closes the wave 71 panels owner question 2."
- Threat model: none.
- Changelog (reports): "Editorial's online dot is a clear green instead of an olive, and someone online on a phone shows a solid phone shape instead of a thin outline."

## Owner questions
1. Editorial's rail icons (add server, explore, music), voice-connected and speaking ring now share the brighter green. Keep, or give Editorial a quieter "success" green separate from online?
2. Editorial's idle tan stays pale; amber like the Lounge themes would read more as "away". Change it?

## Cleanup
`sr_p72_status_ui` dropped; `p72-status-bin` deleted; stack stopped and lock released; no cargo target (none built). Untracked `.scratch/` in the worktree holds only my scripts and logs, never committed.

## Incident
Mid-task I ran `git stash` in my worktree to test the base; stashes are shared across worktrees and I popped what turned out to be the reports builder's one-line change in `apps/web/src/admin/bugs/BugReportsSection.tsx`. I restored it: it is back, unmodified by me, in `p72-reports` (`git status` there shows the file modified, as before); and I reverted it in mine. Reports should check that their file still has their `b.online === null` change.

# Wave 51: overlay (D-540 overlay zoom, the menu layer)

Branch `feat/p51-overlay` (from main `a0b72d01`). Inventory: UTL-SHL-001 follow-up. No Rust, no migrations, no new strings.

## Done

1. **D-540 overlay zoom** (commit "UTL-SHL-001: the overlays follow the member's zoom (D-540)").
   - `settings/user/ThemedOverlayRoot.tsx`: the root stays `display: contents` and carries the zoom as
     `--sr-overlay-zoom` (always set, `1` at 100 %) plus the class `sr-overlay-root`; no real `zoom` on it.
   - `settings/user/overlay-root.css` (imported by the root, so it rides in the drawer and player chunks):
     `.sr-overlay-root > * { zoom: var(--sr-overlay-zoom, 1) }`. The children are the gadget layer, the pin rail, the
     player dock and the modals and menus portalled into the root, so no new stacking context and the z-order stays.
   - `settings/user/overlayZoom.ts`: `zoomFactor`, `overlayZoom()` and `useOverlayZoom()` over the saved prefs.
   - Gadget pixel math (`tools/gadgets/layout.ts`, `store.ts`, `GadgetLayer.tsx`): the layer's viewport is the window
     divided by the zoom (`zoomedViewport`), rectangles stay stored in the layer's px; a drag or resize divides the
     pointer delta by the zoom (`dragRect`), so the title bar stays under the pointer at 150 %; a zoom change re-clamps
     like a window resize. Keyboard moves (16 px steps) are in layer px (24 visual px at 150 %), which is fine.
   - `packages/ui` `useFloating`: menus, tooltips, popovers and the Select list measure in viewport px and translate
     in the floating element's own px (`effectiveZoom` = `currentCSSZoom`), so a menu inside any zoomed box (the
     overlays now, and the Lounge's own zoomed theme root) lands on its anchor instead of 1.5x away.
   - Viewport units (commit "viewport units in the zoomed overlays"): inside a zoomed box `100vw` / `100dvh` are
     zoomed too (Chrome's standardized `zoom`), so at 150 % on a phone the dock ran off the start edge and the drawer
     past the bottom (seen in the first shots). The dock skins (`player.css`, `skins.css`, `skins/backdrop/*.css`) and
     the rail panel divide their viewport lengths by `--sr-overlay-zoom`; `overlay-root.css` does the same for the
     kit's dialogs and menus portalled into the root.
   - Studio (`tools/studio/StudioEditor.tsx`): the crop/redact selection converts the pointer to the canvas's own px,
     so the drawn box matches the pointer under zoom (the region sent to the image was already right).
   - The pin rail needed no math: it is positioned by CSS only.
2. **Menu layer** (commit "UTL-SHL-001: menus, tooltips and popovers open above dialogs"). Menu 950, Tooltip and
   Popover 900, the profile popout (`.lng-pop`) and the TV programme popover (`.tv-pop`) 60 all sat under the modal
   backdrop (1000). All take the Select list's 1050 (above dialogs and drawers, below toasts 1100). Checked the rest:
   no other app popup is portalled or fixed with a low z-index (the other fixed boxes are page chrome: party theatre,
   cinema player, banners). `layers.test.ts` extended; comments in `player.css` / `tools.css` name the new order.

## Tests
- vitest: `packages/ui` `primitives/layers.test.ts`, `primitives/Overlay/zoom.test.tsx` (new: effectiveZoom, the
  translate divided by the zoom); `apps/web` `tools/__tests__/overlayZoom.test.tsx` (new: drag math at 100/150/200 %,
  RTL, clamping, the store viewport, the root's property, the CSS rule, a pointer drag through `GadgetLayer` at 150 %,
  the app popovers' layer), `tools/__tests__/layout.test.ts`, `tools/studio/__tests__/editor.test.tsx`.
- vitest on the areas (inside the lock): `apps/web` `src/tools` + appearance (29 files, 312 passed), `packages/ui`
  `src/primitives` (14 files, 149 passed); plus `drawer.test.tsx`, `menus`, `overlays`, `select` singly.
- Gates: web and ui `tsc --noEmit`, eslint + prettier on changed files (packages/ui is prettier-ignored; e2e has no
  eslint), `check-css-tokens` (clean), `i18n-audit` (0), `pnpm --filter @sideroom/web build`,
  `check-entrance-isolation` (clean), `check-bundle-budget` (all ok), `playwright test --list` (913 tests, 70 files).

## Visual check
Own stack (API 8232, Vite 5232, DB `sr_p51_overlay`, dropped; stack stopped by PID). New local spec
`e2e/tests/p51-overlay-shots.spec.ts` (`SR_P51_OVERLAY=1`, `SR_P51_OUT=...`), 1 passed, 48 shots in
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p51-overlay/` plus `report.txt`:
`drawer-*` (drawer with the guide tool open), `gadget-dock-*` (gadget + pin rail + dock playing), `rail-panel-*`
(rail panel open), `menu-in-dialog-*` (full profile, "More actions" open), each `{desktop,phone}-{lounge-editorial,
lounge-light,lounge-dark}-z{100,150}`. Read: all three themes, both viewports, both zooms.
- Computed zoom 1.5 on the gadget layer, rail and dock at 150 %, 1 on the root (no box).
- Gadget drag of +120,+40 px moved the title bar +120,+40 (+41 rounding) during and after the drag at 100 % and 150 %.
- Dock and drawer inside the window at every size after the viewport-unit fix (phone 150 %: dock 12..363 px, drawer
  bottom 790 of 812).
- The Menu inside the full-profile dialog is topmost (`elementFromPoint` on its first item), z 1050, under its
  trigger on desktop, flipped above it on a phone at 150 % (no room below), correct.
- The Lounge page itself was not changed; the shots enter from a Lounge channel.

## Follow-ups D-553..D-555 (Fable's answers, built the same evening)
- **D-553** (gadget rectangles in the layer's zoomed px): no change.
- **D-554** (commit "the open rail panel comes above gadgets and the dock"): `PinRail` marks the wrap `data-open`
  while a panel shows; `tools.css` raises `.sr-tl-rail-wrap[data-open]` to 911 (908 rail, 909 gadgets, 910 dock,
  911 open rail panel, 1000 modals, 1050 menus, commented). `layers.test.ts` asserts the order.
- **D-555** (commit "the kit token --ui-zoom"): `UI_ZOOM_TOKEN` / `zoomVars` in `packages/ui/src/theme/runtime.ts`
  (default 1 through `var(--ui-zoom, 1)`; deliberately not in `appearanceVars`, so a nested provider inherits it
  rather than resetting it). Modal, Menu, Drawer, Feedback and Overlay module CSS divide their viewport maxima by it.
  `applyPrefs` sets it beside `zoom` (the Lounge, Library and other section roots), `ThemedOverlayRoot` beside
  `--sr-overlay-zoom`; `overlay-root.css` keeps only the zoom rule. `check-css-tokens` scans the five kit files for an
  undivided viewport unit (`scanZoom`, tested). The token name is neutral (no section or theme name).
- Decisions D-553..D-555 pasted after D-540 (no D-541..D-552 rows were present in this branch); D-554/D-555 status
  notes "built in wave 51".
- Tests: `layers.test.ts` (order, every kit viewport length divided, a dialog at 150 % on an 812 px phone fits),
  `Overlay/zoom.test.tsx` (the token), `overlayZoom.test.tsx` (both roots set it, the copies are gone, the rail rule),
  `check-css-tokens.test.mjs` (7 pass). The shots spec now also checks the open rail panel is topmost and the Lounge
  full-profile dialog fits the screen.
- Shots (own stack again, DB recreated and dropped, stack stopped by PID; spec 1 passed): 48 in
  `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p51-overlay-followup/` plus `report.txt`. Read
  `rail-panel-phone-*-z150` (the open panel above the playing dock, wrap z 911, `elementFromPoint` hits the panel in
  all 12 captures) and `menu-in-dialog-phone-*-z150` (the Lounge full-profile dialog, portalled into the Lounge's own
  zoomed root: bottom 782 of 812 px, was past the screen before; the menu topmost at 1050).
- Seen, not changed: on a phone at 150 % the Lounge full-profile dialog is wider than the screen (428 px drawn on
  375, clipped at the inline-start too). The Lounge root itself is laid out at 250 CSS px there; the dialog's width
  comes from its content, not a viewport maximum, so it is outside D-555. Candidate for the next round.
- Gates (inside the lock): web build, `check-entrance-isolation` (clean; its node tests 21 pass), `check-bundle-budget`
  (all ok), vitest `apps/web` `src/tools` + `src/settings/user/__tests__` (58 files, 537 passed), `packages/ui` `src`
  (18 files, 351 passed), `playwright --list` (913 tests), `check-css-tokens` (clean), `i18n-audit` (0), web and ui
  `tsc`, eslint + prettier on changed files.

## Notes
- On a phone at 150 % the layer is 250 px wide: a gadget fills it and covers the rail button (909 over 908, D-553/554
  keep that: the gadget's close button stays on screen); the spec presses the rail button directly there.
- Browsers without `currentCSSZoom` get `effectiveZoom` 1, i.e. the old placement. Only Chromium was shot.
- Owner questions 1-3 of the first round are answered by D-553..D-555; no new questions.

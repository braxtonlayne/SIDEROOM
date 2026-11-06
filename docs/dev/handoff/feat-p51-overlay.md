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

## Not done / notes
- On a phone at 150 % the layer is 250 px wide: a gadget fills it and covers the rail (909 over 908), and the dock
  (910) covers the rail panel. That is the stated layer order, just crowded at that size; the spec presses the rail
  button directly there. See owner question 2.
- Kit dialogs and menus outside the overlay root (the Lounge's own zoomed root) still use unscaled `100dvh` /
  `100vh` maxima, a pre-existing issue at high zoom on small screens; not in this task's scope (owner question 3).
- Browsers without `currentCSSZoom` get `effectiveZoom` 1, i.e. the old placement. Only Chromium was shot.

## Owner questions (for Fable)
1. Gadget rectangles are stored in the layer's zoomed px, so a gadget saved at 100 % opens 1.5x further in at 150 %
   (re-clamped to the window). Default: keep it (one number, no migration); alternative: store viewport px.
2. Phone at 150 %+: should the dock or a gadget yield to the pin rail (e.g. the rail above both), or keep 908 < 909 <
   910? Default: kept.
3. Divide the kit's dialog/menu viewport maxima by the zoom everywhere (needs a kit-level zoom token, which would be a
   new `--ui-*` token)? Default: only inside the overlay root this wave.

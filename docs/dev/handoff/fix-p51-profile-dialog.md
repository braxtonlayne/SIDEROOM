# Handoff: fix/p51-profile-dialog (XC-VIS-004, the Lounge full profile at zoom)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p51-pdlg`, branch `fix/p51-profile-dialog` (base integration/phase51). Not pushed.

## Cause
On a phone at 150 % the full-profile dialog was 437 px drawn on 375 (right edge 460, clipped at the start too); at 200 %
618 px. The Lounge root is zoomed and laid out at 250 CSS px, so D-555's `--ui-zoom` maxima were not the problem: the
kit's `.backdrop` is a one-track grid (`place-items: center`) whose auto track grew to the dialog's min-content, and
the min-content came from unbreakable text (the `@handle`, the tab strip). `width: 100%` of an auto track does not cap it.

## Done
- `packages/ui/src/primitives/Modal/Modal.module.css`: `.backdrop { grid-template-columns: minmax(0, 1fr) }` and
  `.dialog { min-width: 0 }`. Every kit dialog now takes its width from the viewport, never from its content (the
  profile dialog is the only one seen at fault; Study and Arcade do not use it, `FullProfile` is Lounge-only).
- `apps/web/src/lounge/profile/profile.css`: `.lng-pop-handle` / `.lng-pop-activity` wrap anywhere (the handle ran past the
  banner even at 100 %). `profile-p11.css`: `.prf-full-actions` wraps (the Add friend button was cut off at 150 %).
- Test: `packages/ui/src/primitives/Overlay/zoom.test.tsx` (the two rules). Shot spec `e2e/tests/p51-pdlg-shots.spec.ts`
  (local, `SR_P51_PDLG=1`): opens the dialog in Editorial, Lounge Light, Lounge Dark, desktop 1280x800 and phone 375x812,
  at 100/150/200 %, asserts the box is inside the viewport.

## Shots (`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p51-pdlg/`, `before/` is the old code; `report.txt` has the boxes)
Before: phone z150 right 460, z200 right 618 (viewport 375). After: right 352 and 345, left 23 and 30, bottom inside,
desktop unchanged (z150 39..1241 of 1280, z200 30..1250). Looked at phone z100/z150/z200 and desktop z150 in the three
looks. Seen and left: on a phone the tab strip is a horizontal scroller (clipped by design) and the handle wraps by
character when only ~100 px are left at 200 %.

## Tests
web and ui tsc, build, `check-entrance-isolation`, `check-bundle-budget`, `check-css-tokens`, `i18n-audit`, ui vitest, `playwright --list`,
prettier/eslint on changed files (see the final reply for results). No Rust, no i18n strings.

## Owner questions
None.

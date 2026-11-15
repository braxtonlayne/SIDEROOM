# Wave 77 handoff: picker (SEC-AUD-301, D-799)

Branch `fix/p77-picker` (base `integration/phase77`, from main `90e94c45`).

## Done
- `packages/ui/src/primitives/Overlay/Overlay.tsx`: `Popover` takes an optional `anchor?: HTMLElement | null` ("The element the panel is placed against; the trigger when absent"); `useFloating({ open, anchor: anchorProp ?? anchor, placement })`. Nothing else changed (ARIA, toggle, Escape, focus return, outside press).
- `apps/web/src/lounge/Composer.tsx`: `const [box, setBox] = useState<HTMLDivElement | null>(null)`, `ref={setBox}` on the live `.lng-composer`, `anchor={box}` on the GIF, sticker and emoji `Popover`s. `placement="top-end"`, `clearRef` and the locked composer are untouched. No CSS, no string, no fragment.
- Tests: `packages/ui/src/primitives/p77PopoverAnchor.test.tsx` (new file, not a case in `overlays.test.tsx`); `e2e/tests/p77-picker-shots.spec.ts` (local-only, `SR_P77_PICKER=1`; `SR_P77_BASE=1` runs only the base measurement).
- Commits: the code change, the vitest, the spec (and this handoff).

## Confirmed on the base (run, not simulated)
I put the base `Composer.tsx` and `Overlay.tsx` back into the working tree (`git checkout 90e94c45 -- <the two files>`, no stash), ran the spec's base case against the same stack, then restored my change. Emoji picker, Editorial, panel bottom vs `.lng-composer` top:
- 1280x800, five-line draft: panel bottom 734.0, composer top 642.0 (panel covers 92px of the draft).
- 375x812, empty composer (placeholder wraps to two lines): panel bottom 689.0, composer top 663.0 (26px over the field).
Shots `base-emoji-desktop.png`, `base-emoji-phone.png` (the picker over the text) are in the shot folder; `base-measurements.txt` has the numbers.
vitest: on the base `anchor` is not a prop, so the first case of `p77PopoverAnchor.test.tsx` does not compile there (not run on the base). With the change both cases pass: the panel's `translate` follows an anchor's faked rect (`translate(400px, 492px)`), and without `anchor` follows the trigger's span (`translate(50px, 292px)`).

## Measured after (every case in the spec asserts it)
Panel bottom is exactly 8px above the composer top in all of the 36 runs, e.g. desktop: five-line draft 634.0 vs 642.0, empty 722.0 vs 730.0, one-line sticker 722.0 vs 730.0, thread (three lines) 678.0 vs 686.0. Phone: empty 655.0 vs 663.0, five lines 589.0 vs 597.0, one line 677.0 vs 685.0, thread 633.0 vs 641.0. Desktop: panel end edge equals composer end edge (1020 / 1020; thread pane 1264 / 1264). Phone: panel left 8, right 367 (screen 375): 8px to spare. No horizontal overflow in any shot. All numbers in `measurements.txt`.
Click-through (asserted, every theme and size): picking an emoji with a five-line draft puts it in the text, keeps the draft, and the composer textarea keeps focus; Escape closes the picker and the emoji button has focus; a press on the message list closes it.

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p77-picker`: 36 shots (emoji over an empty composer, emoji over a five-line draft, GIF over a five-line draft, sticker over a one-line draft, emoji in the thread pane with a three-line draft; each at desktop 1280x800 and phone 375x812 in Editorial, Lounge Light, Lounge Dark; account theme set by `PATCH /users/@me/settings` and localStorage) plus 2 base shots. I opened all 38 and looked at each: the picker ends above the composer, every draft line is visible, edges as above, themes render correctly. The GIF picker showed "GIFs are not set up on this server yet" (no GIF source) and the sticker picker "This server has no stickers yet": placement is what is shot. In the GIF shots the draft's last line ends with the emoji picked earlier in the same run.

## First paint and checks (one lock round)
- `git grep` of app, components, public, router: none of them names `Popover` or `Tooltip`; they import other kit parts (Button, Notice, ThemeProvider...). The build and the budget answer the question below.
- `pnpm --filter @sideroom/web build`: ok. `check-bundle-budget.mjs`: all ok; the `app` (first paint) chunk is 210.9 KB gzip against a 300 KB budget (wave 75: 215,903 bytes), so the 8 KB room rule is met with room to spare; public pages: landing 115.8 / 120, auth 121.4 / 150, invite 113.5 / 120, profile 115.1 / 150, guest-play 115.5 / 120 KB. `check-entrance-isolation.mjs`: every public surface clean. `check-staff-delivery.mjs`: ok.
- Local: `tsc --noEmit` web, ui (own package) and e2e clean; eslint clean on both source files; prettier clean on Composer.tsx and the spec (the `packages/ui` package's files do not pass the root prettier config even on the base, e.g. `overlays.test.tsx`, so I did not reformat); `node scripts/i18n-audit.mjs --max 0`: TOTAL 0; `check-doc-links.mjs` ok; the ui vitest files `p77PopoverAnchor` and `overlays` (17) pass; the composer test files `p15ComposerExtras`, `p17ComposerBackdrop`, `composerGaps` pass.
- `composerActivity.test.tsx` fails ("Unable to find role=button name 'Add to message'") on the base and with my change alike: pre-existing, not mine. (Under heavy load `composerGaps` also timed out three cases once; alone it passes 14/14.)

## Findings
- ThreadPane draws the same `Composer`: one change covers both (shot: `thread-emoji-*`).
- `lounge/rings/RingComposer.tsx` has no emoji, GIF or sticker picker and no `Popover` (grep): nothing placed the same way.
- Docs: `git grep -i popover -- docs packages/ui/README.md packages/ui/src/stories` finds no living doc or story that lists `Popover`'s props (docs/10 only names it among the primitives; the rest are historical handoffs and the decisions log); nothing to update.
- Existing assertions changed: none. `git stash list` was not touched.
- Cleanup: stack stopped by PID, `sr_p77_picker_ui` dropped, `p77-picker-bin` deleted, lock released.

## Proposed inventory row
SEC-AUD-301 (low, web): the composer's emoji, GIF and sticker pickers were placed above their button on the composer's last line, so on a phone (placeholder wraps) or with a multi-line draft they covered the upper lines of the message. Fixed (D-799): `Popover` takes an optional `anchor`; the composer passes its own box, so the pickers open above the whole composer with their end edge on the composer's. Test: `p77PopoverAnchor.test.tsx`, `p77-picker-shots.spec.ts`.

## Changelog sentence
The emoji, GIF and sticker pickers now open above the whole message box, so they no longer cover what you are writing (on a phone, or with a long draft).

## Owner questions
None.

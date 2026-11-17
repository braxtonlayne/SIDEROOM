# Wave 93, tidepool: SEC-AUD-418 (D-926), the Tidepool skin passes the 44 px hit test on a phone

2026-11-17. Clock: start 12:54, first lock round 13:15 to 13:23, second 13:35 to 13:38 (released). Commits: `SEC-AUD-418: Tidepool phone targets`, then spec-only commits. **The CSS in the last commit is the CSS measured in both rounds** (no rule changed between the first run and the last).

## Base (375 x 812 touch, queue of three tone tracks, first title 57 characters; spec `e2e/tests/p93-tidepool-shots.spec.ts`, helper `hitTestAll`, phone block deleted from the live sheet)
17 of 18 fail, 0 not reached; only Pause (62) passes. Boxes and takers: Library, Equalizer, Skin 92 x 36 (takers: the "Player menu" nav, the body); corner Move and Close 24 x 24 (taker: the body); Seek 293 x 18 (taker: the screen grid); Shuffle 42 x 42, Previous and Next 52 x 40 (the deck, the slot); Mute 30 x 30 (taker below: the Volume input); Volume 56 x 16 (taker above: the Mute ball); Repeat 108, Stop 71, Drift off 114, each 18 high (takers: the deck's slots above, the strip below); queue rows 277 x 27. Document 375/375, dock 359/359, skin 359/359; skin height 658; dock does not scroll. Queue title contrast min 5.97 (helper, text hidden, ground from the shot): the wing is opaque under the rows, nothing reads through it (shots opened), so the wing is left as it is.

## Rules as built (`tidepool.css`, one `@media (max-width: 720px)` block at the end, with `@container (max-width: 38rem)` inside it so it applies only where the skin's narrow layout does; every selector under `.sr-ply-tidepool`)
- Pills: `min-block-size: 2.75rem` (grown, room above and below).
- Corner gems and Mute ball: drawn size kept, centred 44 px `::after`; gems 1.25rem apart (centres 44 px apart), crown `min-block-size: 2.75rem`, capsule margin made room; Mute and Volume 0.5rem apart so the two targets do not meet.
- Seek: input 44 px tall, `margin-block: -0.8125rem` keeps the tube's height, `position: relative` so it paints over the wave.
- Deck: Shuffle 44 x 44, Prev and Next 44 high. Volume: input 44 px, the tube (rings, gel, fill) moved onto the track pseudo-element (`-webkit-slider-runnable-track`, `-moz-range-track`), the inset shadow approximated by a gradient layer; `margin-block-end: -0.875rem` keeps the tube where it was.
- Strip chips (Repeat, Stop, Drift off): 44 px high, pill or hover drawn 22 px high (transparent block border, `background-clip: padding-box`; the Drift pill's outer shadow became `filter: drop-shadow`); strip padding and row gap adjusted.
- Queue rows: `min-block-size: 2.75rem`, centred; queue `max-block-size` 11.5rem so three rows do not scroll inside the wing.

## After (same round, same run)
18 of 18 pass, 0 fail, 0 not reached. Boxes: pills 92 x 44; gems 24 x 24 (target 44); Seek 293 x 44; Shuffle 44; Prev, Next 52 x 44; Pause 62; Mute 30 (target 44); Volume 56 x 44; chips 44 high; rows 277 x 44. Gaps between neighbouring control boxes: Mute to Volume 8 px, rows 2 px, chips 6 px beside, gems 20 px beside (targets 44 apart): no two targets overlap. Touch taps at centre, 20 px above and below: 46 taps, 0 failed (Pause, queue rows 2 and 3, Prev, Next, Volume, Shuffle, Mute, Repeat, Drift off, EQ, Move, Skin pill, Close then reopen, Stop). Document 375/375, dock 359/359, skin 359/359; nothing leaves the skin's box. Skin height 658 -> 747 px (+89); the dock now scrolls by 8 px (747 against 739), one scroller, third row whole inside the dock at its end (the dock builder's cap changes that). Reduced motion: running animations 15 with motion allowed, 0 reduced (base 15 / 0). Queue title contrast min 5.97. 1280 x 800: 18 controls, max box difference 0 px against the base, height 498 both.

**Seek by touch (spec note, not a page fault):** on this harness the audio element snaps back to the start after any seek (a direct `audio.currentTime = 20` does the same on the base page, `measure.json` `baseSeek` and `afterSeek`), so the spec checks that the Seek input fires an `input` event with the tapped value at centre and 20 px above and below (it does, 3 of 3). Not isolated further.

## Listed, not built
Between 625 and 720 px viewport the wide layout shows (the container query is above 38rem) and the block does not apply; not measured. Firefox track rules are written but not measured (Chromium only).

## Lazy chunks
`tidepool.css` is imported only by `TidepoolSkin.tsx`, which `PlayerHost.tsx` loads lazily (`git grep -n`; plus `__tests__/backdrop-skins.test.tsx`). No web build run. `TidepoolSkin.tsx` unchanged.

## Shots (`visual-sweep-shots/p93-tidepool`, 5 PNGs, all opened, plus `measure.json`)
`base-first-375`, `base-end-375`, `after-first-375`, `after-end-375`, `after-1280`. The base has no scroll (the skin fits), after scrolls by 8 px. Nothing cut or overlapping in them.

## Checks
prettier on the css and spec, e2e `tsc`, `check-css-tokens`, `i18n-audit --max 0` (0), `check-doc-links`, spec passes (`--project=chromium`).

## Commander's command (from `e2e/`, stack with the sweep world, API 8718, Vite 5718, metrics 9418, `SR_ADMIN_REQUIRE_REAUTH=0`)
`SR_E2E_BASE_URL=http://localhost:5718 SR_P93_TIDEPOOL_ON=1 SR_P93_TIDEPOOL_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p93-tidepool npx playwright test tests/p93-tidepool-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (`SR_P93_TIDEPOOL_BASE=1` runs the base only).

## Proposed rows
- SEC-AUD-418 closes: "Tidepool on a phone: 17 of 18 controls failed the hit test (boxes 16 to 42 px, takers the body, the deck, Volume under Mute); now 18 of 18 pass, no two targets overlap, 46 touch taps do their job, no sideways scroll, 1280 unchanged (0 px); the skin is 89 px taller (D-926)."
- Open (new): Tidepool between 625 and 720 px viewport width is not covered (wide layout).

Changelog sentence: The Tidepool player skin has full-size controls on a phone.

For wave 94 (Valve, Stack): what worked: a real 44 px box with the drawing kept by a transparent block border and `background-clip: padding-box`, a `::after` for round buttons with their neighbours 8 px away, and a range input made 44 tall with its tube moved onto the track pseudo-element; what to avoid: negative margins on rows that can wrap, and `box-shadow` on a box whose drawn part is smaller than its box (use `filter: drop-shadow`).

Owner questions: none.

## Cleanup
Stack stopped (8718 / 5718 / 9418 free), `sr_p93_tidepool_ui` dropped, `p93-tidepool-bin` deleted, lock released. No stash. `p92-base-bin` and `p80-base-bin` untouched.

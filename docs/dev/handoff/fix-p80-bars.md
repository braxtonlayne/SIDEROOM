# Wave 80 bars handoff (SEC-AUD-320, SEC-AUD-321; D-821, D-822)

T = 07:30 on 2026-11-16. No new item from 09:30, checkpoint 09:50, finished 10:15, hard limit 10:30. My own clock: started 07:28, hook committed 07:31, lock taken 07:41, stack stopped and lock released 07:45, handoff 07:46. Everything finished well inside the window.

## Done
- **SEC-AUD-320.** `apps/web/src/study/useCurrentLinkInView.ts`: `useCurrentLinkInView(ref: RefObject<HTMLElement | null>, selector: string, key: unknown): void`. It is built on the existing `study/scrollBarTo.ts` (no second helper): effect keyed on `ref`, `selector`, `key`; finds the link, calls `scrollBarTo` (moves only the bar's `scrollLeft` by the difference of the two boxes, with 16 px to spare), runs at once and again after `document.fonts?.ready` (a `live` flag cleared on cleanup). No `scrollIntoView`, no string, no dependency.
  - Commits: hook and test `3884abbf`; call sites `dbc4e457` (Cinema `SectionBar`: same selector, key `path`; Library `SectionBar`: same selector, key `` `${pathname}|${mobile}` `` so it still re-runs when the viewport crosses the phone breakpoint, as the old effect did); spec (see below).
  - Where it is loaded: only by `cinema/Shell.tsx` and `library/Shell.tsx`, which already import `RoomsNav` and `scrollBarTo` (`git grep "study/RoomsNav"`: arcade, cinema, library, lounge/Chrome, showroom, study). Lazy chunks only, no web build.
  - Vitest `useCurrentLinkInView.test.tsx` (6 cases, rectangles faked): in view leaves `scrollLeft`; past the end moves forward; before the start moves back; `scrollIntoView` spy never called; runs again when `key` changes and not when it stays; `dir="rtl"` moves the right way (checked: the sign is right because it is box differences). **Watched fail with the hook body empty (4 of 6 failed; the two "leaves alone" cases pass trivially), then 6/6.** `study/__tests__/study.test.tsx` 6/6. Web typecheck, eslint, prettier clean.
- **Base vs after (375x812, shipped and light, `sideroom` / `lounge-light` read back from the root; same stack, base = the two Shell files from `c8982549`, after = my change).** The two runs' measure files compared key by key: **0 differences** (document `scrollWidth`/`clientWidth`, `scrollLeft`, `scrollTop`, the three room link boxes, the current area box, the bar's `scrollLeft`/`scrollWidth`/`clientWidth`). No look changed.

| page | theme | Media/Reading box (left, right) | bar scrollLeft | doc scrollLeft / scrollTop |
|---|---|---|---|---|
| /cinema | shipped | Media 321..356 | 0 (of 435) | 0 / 0 |
| /cinema title | shipped | Media 321..356 | 0 | 0 / 0 |
| /library | shipped | Reading 314..359 | 50 (of 425) | 0 / 0 |
| /library/s/9001 | shipped | Reading 314..359 | 50 | 0 / 0 |
| /cinema | light | Media 321..359 | 18 (of 459) | 0 / 0 |
| /cinema title | light | Media 321..359 | 18 | 0 / 0 |
| /library | light | Reading 310..359 | 74 (of 449) | 0 / 0 |
| /library/s/9001 | light | Reading 310..359 | 74 | 0 / 0 |

  Click-through (phone, light), from the Cinema and from the Library, each of Shelves, Media, Reading pressed in turn: the pressed link is inside the bar afterwards (box within 0..375) and the document did not move sideways (`scrollLeft` 0, `scrollWidth` 375), in both rooms, base and after.
- **SEC-AUD-321: measured whole, nothing built** (row closes as "measured whole"). No Arcade or Study page scrolls sideways; no element is past the right edge; the three room links are inside the viewport everywhere. There is no per-game page route in the Arcade (only `/games` and `/games/emulator`; a game opens from its card), so "a game's page" does not exist: measured the home with a featured game, Playable now cards and Coming soon instead. The Study area is the Study home with the shelf "Home" selected and 7 files (the page opens the shelf itself, so the "home" and "area" shots are near-identical).

| page | size | theme | scrollWidth / clientWidth | past the edge | rule |
|---|---|---|---|---|---|
| Arcade home | 375 | shipped (`sideroom`) | 375 / 375 | none | none |
| Arcade home | 375 | light (`lounge-light`) | 375 / 375 | none | none |
| Arcade home | 1280 | shipped / light | 1280 / 1280 | none | none |
| Emulator | 375 and 1280 | shipped / light | 375/375, 1280/1280 | none | none |
| Study home | 375 and 1280 | Lounge Dark / Lounge Light | 375/375, 1280/1280 | none | none |
| Study area (7 files, long names) | 375 and 1280 | Lounge Dark / Lounge Light | 375/375, 1280/1280 | none | none |

  Room link boxes at 375: Arcade 19..257; Study 16..305 (the Study bar wraps to two rows, the Arcade's brand sits on its own row). Both CSS files are imported by `arcade/ArcadeShell.tsx` and `study/StudyPage.tsx`, lazy chunks: no web build.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p80-bars/after/`: 24 PNGs (8 phone Cinema/Library, 8 Arcade/Emulator, 8 Study), **all 24 opened and looked at by me**; `base/` holds the base run's 24 (not each opened: the measure files are identical). Findings from looking, none a sideways-scroll fault and none mine to fix: (1) in the light Arcade at 1280 the card title "Whiteboard" breaks mid-word ("Whiteboar / d") in the fourth Playable card; (2) the light Cinema title page at 375 shows a light gradient across the right half of the dark hero (the kept-dark surface; dark builder's area); (3) on the Cinema bar the third group's last link ("Reading") is cut at the right edge at load, by design (D-817: the scrolling bar fades; Media is the current area and is inside).

## Not done / notes
- No Rust, no migration, no string, no CSS file changed. No `git stash` used; I saw none. Stack stopped by PID, `sr_p80_bars_ui` dropped, `p80-bars-bin` deleted, lock released. Spec `e2e/tests/p80-bars-shots.spec.ts` (local only, `SR_P80_BARS=1`, `SR_P80_BASE=1` for the base name) typechecks and is prettier-clean; `node scripts/i18n-audit.mjs --max 0` TOTAL 0.

## Proposed inventory rows
- SEC-AUD-320: one `useCurrentLinkInView` hook (`study/`) brings the current link of a scrolling bar into view by the bar's own `scrollLeft`, never `scrollIntoView`; Cinema and Library bars use it (D-821). Closed; vitest, base/after figures identical.
- SEC-AUD-321: no Arcade or Study page scrolls sideways at 375 or 1280 (D-822). Closed as measured whole.

## Changelog sentence
None a member would notice.

## `docs/10-design-system-and-theming.md` (D-822)
"A bar with two groups (a room's rooms and its areas) scrolls inside itself on a phone; a bar with one group (the Arcade's and the Study's three rooms) wraps."

## Owner questions
1. The "Whiteboard" card title breaks mid-word in the light Arcade at 1280: fix in a later Arcade pass (not a sideways-scroll fault)?

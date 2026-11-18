# Wave 104, reader (D-998; SEC-AUD-472, 473, 474)

Clock: start 14:13; spec committed 14:16; asked for a slot from 14:18, got it at 14:22:58 (about 5 minutes of waiting, `mount` and `chip` held it); round 14:22:58 to about 14:28; released 14:28. Handoff written 14:35.

## Done (commits on `fix/p104-reader`, base `ccb3bacc`)

- `07b28c69` D-998 reader shots spec (`e2e/tests/p104-reader-shots.spec.ts`, new).
- `31046c46` D-998 label gap: `.lib-select:not(.lib-rate) > span:first-child:not(.lib-sr-only) { margin-inline-end: 8px }`, one rule at the end of `library.css`.
- `e68b252f` D-998 reader bar at 375: one `@media (max-width: 480px)` block at the end of `library.css`: `.lib-reader-top .lib-reader-title { min-inline-size: 0 }` and `.lib-reader-top label.lib-chapter-select { flex: 0 1 auto; min-inline-size: 96px }`.
- `ed778941` D-998 end card: `.lib-reader:not([data-bg='white'], [data-bg='sepia']) .lib-endcard` joins the selector list of the bars' token block (`library.css:982`).
- `d1e4d1ba` D-998 end card eyebrow gold (a finding of step 1, see item 2) and the spec compares hit tests with the base only.
- `Reader.tsx` is unchanged. No string, no kit file, no other Library file.

## Step 1 (confirm first; base = tree at `ccb3bacc`, one STACK-ROUND, dev mode)

### 1. The bar at 375 (shipped theme `sideroom`): CONFIRMED

Every `.lib-icon-btn` is 36 x 36 (not 44). Box x and w, computed `flex`, `min-inline-size`:

| child | base x | base w | flex | min-inline | after x | after w |
|---|---|---|---|---|---|---|
| back arrow | 16 | 36 | 0 1 auto | 36px | 16 | 36 |
| title `h1` | 60 | 0 | 1 1 0% | auto | 60 | 0 (min 0) |
| chapter picker label | 68 | 220 | 0 1 auto | auto | 68 | 115 (min 96) |
| bookmark | 296 | 36 | 0 1 auto | 36px | 191 | 36 |
| comments | 340 (ends 376) | 36 | 0 1 auto | 36px | 235 | 36 |
| bookmarks | 384 (ends 420) | 36 | 0 1 auto | 36px | 279 | 36 |
| settings | 428 (ends 464) | 36 | 0 1 auto | 36px | 323 (ends 359) | 36 |

Bar `scrollWidth` 464 against `clientWidth` 375 at base (the viewport scrolled right in the base shot: the arrow and title were off the left edge); after 375 / 375. Past 375 at base: Comments, Bookmarks, Reader settings. The title was already 0 wide at base (basis 0, nothing left for it): the picker's 220 px was the cause, as the task guessed. Bar height 60 at 375, 56 at 1024 (equal base and after).

At 320 (`p71-panels-shots` and `player-layout` shoot it): base `scrollWidth` 464, four buttons past 320; after 340: the picker is at its 96 px floor and Reader settings still ends at 340 (20 px past). The buttons alone are 5 x 36 + 6 x 8 + 32 = 260 of 320, so 60 px is left for the picker, under the task's 96 floor. I kept the floor (D-998 names 375); see owner question 1.

Hit test over the top bar at 375 (default reach): base 1 pass / 5 fail, after 1 pass / 5 fail. All five fails are the 36 px buttons: the point 21 px above or below the centre is outside the 36 px box and the taker is the bar itself (`header.lib-reader-top`). The picker (44 px) passes. So "0 fails after" cannot be met without making the buttons 44 px, which the task forbids ("every bar button's size must not change"): this is a button-size fault of `.lib-icon-btn` (`library.css:206`, outside my row), not of the bar's width. Not worse than the base. 44 px buttons would need 5 x 8 = 40 px more and leave the picker 35 px: a second row or a menu is then the cure, the planner's to decide.

### 2. End card under `lounge-light`: CONFIRMED (the comments button, the rating field; and the eyebrow, not on the task's list)

The end card (`section.lib-endcard`) is inside `.lib-reader`, which is not one of the four elements of the token block (`.lib-reader-top`, `.lib-reader-bottom`, `.lib-sync-notice`, `.lib-pdf .lib-pdf-bar`), so it read the Lounge Light tokens (`--ui-text` `#313338`). Ground: the stage, `rgb(0,0,0)` (`data-bg=black`).

| part | colour at base, light | ground | ratio base light | ratio base sideroom | ratio after light |
|---|---|---|---|---|---|
| comments button text | `#313338` | stage black | 1.7 | 16.9 | 16.9 |
| rating field text ("Not rated") | `#313338` | the field, `rgb(242,243,245)` at base (a white box on the black card); `#1c1d18` after | 1.7 as the spec lays it on the stage | 16.9 | 16.9 |
| "Rate this chapter" label | light | stage | 16.9 | 16.9 | 16.9 |
| eyebrow "End of chapter N" | `rgb(71,82,196)` (Lounge Light accent, via `--sr-gold`) | stage | 3.3 | 9.9 | 9.9 |

Same at 1024 and 375 (identical figures). The eyebrow is `color: var(--sr-gold)`, which the token block does not restate; one rule in my end block states `--sr-gold: #c2b08f` on the end card only (same stage condition), so the eyebrow reads 9.9:1.

Scope choice: the dark tokens go to the end card only on the black and grey stages (`:not([data-bg='white'], [data-bg='sepia'])`): on a white or sepia stage the theme's dark text reads and dark tokens would not (a reader preference, `prefs.background`). Under `sideroom` a white or sepia stage with light text is the base's own fault and I did not touch it (owner question 3).

### 3. Label gap: CONFIRMED

`labelGap` (label text right edge to field left edge), base to after, equal at 1024 and 375: Series Edition 0 to 8; My library Sort by 0 to 8; Lists new and edit "Who can see it" 0 to 8; review "Rating with this review" 8 to 8 (`.lib-rate`, a flex box with its own 8 px gap, left out); Rankings Kind has no visible text (sr-only label), nothing to measure.

### 4. The chevron on the bar under `lounge-light`: no change

Measured under Lounge Light (and Side Room, identical): chevron `rgb(169,164,147)` (`#a9a493`) on the select's ground `rgb(28,29,24)` 6.8:1, on the bar 8.0:1. Wave 103's libshots figures (6.8 and 7.99) are true. The 2.6:1 estimate is wrong because it read the theme's own muted text; the bars' token block (`library.css:981`) restates `--ui-text-muted: #a9a493` on the bar, and the chevron is drawn from that token. Above 3:1: no change.

## Step 2 against the two lists (every figure base to after, from `compare.json`; the last spec run: `problems: []`)

May change (and did): at 375 the title stays 0 wide; the picker 220 to 115 (320: 96); x of bookmark, comments, bookmarks, settings (296, 340, 384, 428 to 191, 235, 279, 323). End card under `lounge-light`: text and field colours. A `.lib-select` label: gap +8, label width +8 and the x of what follows on its line (Edition label 215.2 to 223.2; My library Sort 205.5 to 213.5).

Must not change, all held (0.5 px): the bar at 1024 (every child's x, y, w, h equal, picker 220 and 220); bar height 56 at 1024, 60 at 375 and 320; every bar button 36 x 36; the bottom bar (x, w, h and its children) equal, hit tests equal; the end card under `sideroom` (colours and every box equal); the end card's boxes under `lounge-light` (equal); `.lib-select` field heights (40 at 1024, 44 at 375) and font size (14px) equal; no row newly wraps (label heights equal, label text lines 1 to 1: Edition, My library Sort, Lists visibility new and edit, review rating; Rankings Kind has no text); the Browse toolbar `.lib-sort` box equal (1024: 273,262 160x40; 375: 16,270 160x44); no page scrolled sideways. The hit test over the top bar: not worse (1/5 to 1/5); the bottom bar not worse.

## Who imports (git grep)

`Reader.tsx` (unchanged): `apps/web/src/library/routes.tsx:15` through `lazyNamed(() => import('./reader/Reader'))`: the Library chunk. `library.css`: `LibraryRoot.tsx:9`, the Library chunk. Neither is loaded by first paint or by a public page. Dependants of the rules: the `.lib-select` span rule reaches the five `label.lib-select` (SeriesPage Edition, Lists visibility, Pages Sort, Rankings Kind, SeriesSocial review rating) and nothing else (`.lib-select-block` and `.lib-sort` are different classes); the 480 px block reaches only `.lib-reader-top`'s title and its `label.lib-chapter-select`; the token block entry and the eyebrow rule reach `.lib-endcard` only (used by `Reader.tsx`'s `EndCard`; the p101 and p102 library specs read `.lib-endcard` as a container only).

## STACK-ROUND

One round on one database, base and after: acquire at 14:22:58 (`status` read: `reader 14:22`), stack on 8804 / 5804 / 9504, `e2e/.state` removed, the sweep world seeded (no FAIL), base (library.css put back with `git checkout ccb3bacc -- apps/web/src/library/library.css`, restored with `git checkout HEAD -- ...`), after run 1 (3 problems, named below, fixed), after again on the same stack: 1 passed, `problems: []`; stack stopped by PID, ports free, released about 14:28 (`status` after: `loungeshots 14:27`, no hold of mine). Waited about 5 minutes for the slot. The run 1 problems were: the 5 hit-test fails (bar buttons are 36 px, same as base: the spec now compares with the base only), the eyebrow at 3.3:1 (cured), and the 320 overflow (the 320 row is informational). The `sr-cli staff appoint` line in the log is the non-fatal line earlier specs print too.

## TYPECHECK

Not needed: no `.ts` or `.tsx` under `apps/web` changed (only `library.css`). e2e `npx tsc --noEmit -p tsconfig.json` from `e2e/`: exit 0 (about 14:26, after the last spec edit). `prettier --check` on `library.css` and the spec: clean. The web eslint config ignores `e2e/` ("File ignored because outside of base path"), so the spec has prettier and tsc only; `Reader.tsx` is unchanged, so no eslint run. `vitest run src/library/__tests__/reader.test.tsx`: 16 passed (14:28). `node scripts/check-css-tokens.mjs`: exit 0 (73 files clean). `node scripts/i18n-audit.mjs --max 0`: TOTAL 0. `node scripts/check-doc-links.mjs`: "doc links ok".

## Shots

Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p104-reader`: 20 PNG (11 after, 9 base). I opened the 11 after-shots and the base shots named; `data-theme` was read back before each (`sideroom` or `lounge-light` as set).

- `base-reader-bars-375-sideroom.png`: the page is scrolled right (the document scrolled to show the settings button): the back arrow and title are off the left edge and the bar runs past the right edge. This is the fault.
- `reader-bars-375-sideroom.png`: arrow, picker ("Chapter ..." cut by the kit's ellipsis, chevron clear of the text), bookmark, comments, bookmarks, settings all whole inside 375; bottom bar whole.
- `reader-bars-375-light.png`: the same, the bar dark under the light theme.
- `reader-bars-1024-sideroom.png`, `reader-bars-1024-light.png`: full title, 220 px picker "Chapter 1: p104-rd first" uncut, four buttons; the boxes equal the base's.
- `base-reader-end-1024-light.png`: the base fault: "0 comments" button text near-invisible dark on black, "Not rated" a white field on the black card, blue eyebrow and blue Next button.
- `reader-end-1024-light.png`, `reader-end-375-light.png`: after: "0 comments" in light text, "Not rated" in the dark `#1c1d18` field with light text, gold eyebrow and gold Next button. The card's layout equals the base's.
- `reader-end-1024-sideroom.png`: as the base (card and colours unchanged).
- `series-edition-1024-sideroom.png`: "Edition" has a visible gap before the field.
- `lists-visibility-375-sideroom.png`: "Who can see it" has a visible gap before "Only me", on one row.

## STALE (earlier specs this change makes untrue; never edited)

`e2e/tests/p103-library-shots.spec.ts`, if run against a base taken before this change: lines 1054 to 1057 (the picker's selected text "is cut": at 375 it is cut on purpose now) and lines 1063 to 1066 ("at 375, narrower than the base by more than 36": the picker is 115 against 220). Both name something D-998 changed on purpose: stale expectation. Nothing else of p103 is affected as far as I read.

## The commander's command (from `e2e/`, on a stack with the sweep's world, `SR_ADMIN_REQUIRE_REAUTH=0`, `DATABASE_URL` and `SR_CLI_BIN` set; the base first with `library.css` at `ccb3bacc`, then HEAD)

```
SR_E2E_BASE_URL=http://localhost:<vite> SR_P104_READER_ON=1 SR_P104_READER_PHASE=base \
SR_P104_READER_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p104-reader \
npx playwright test tests/p104-reader-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
# then the same without SR_P104_READER_PHASE=base on the merged tree
```

## Closing texts for the rows

- SEC-AUD-472 (reader top bar wider than the viewport at 375): fixed in wave 104 (D-998): at 480 px and under the title gives way, then the chapter picker shrinks from its 220 px cap to no less than 96 px; at 375 the bar is 375 wide and the picker 115. At 320 the bar is still 20 px too wide (buttons 36 px, picker held at 96), and the five 36 px buttons still fail the 44 px hit test as in the base: both stay open (owner questions 1 and 2).
- SEC-AUD-473 (reader end card under Lounge Light): fixed (D-998): the end card takes the bars' dark tokens on the black and grey stages; "0 comments" and the rating field read 16.9:1 (were 1.7:1), the eyebrow 9.9:1 (was 3.3:1). Unchanged under Side Room; its boxes unchanged under every theme.
- SEC-AUD-474 (`.lib-select` label touches its field): fixed (D-998): the label's text span has an 8 px end margin (not `.lib-rate`, which has its own 8 px gap).
- The chevron question: wave 103's 6.8:1 (on the select) and 8.0:1 (on the bar) are true; the 2.6:1 estimate read the theme's muted text, not the bars' restated token. No change.

## Changelog sentence

On a phone the Library reader's top bar now fits the screen (the title and then the chapter picker give way), the end of a chapter reads under a light Library theme, and a select's label keeps a gap from its field.

## Owner questions

1. At 320 the bar is still 20 px too wide with the 96 px floor. Lower the floor to 56 px at 340 px and under, or give 320 a second row? (Default I would pick: 96 at 361 and over, 56 below.)
2. The 36 px bar buttons fail the 44 px hit test at 375 (base and after alike); 44 px buttons would leave the picker 35 px: a second row or a menu for the bar? The planner's.
3. Under `sideroom` a white or sepia reader background puts the end card's light text on a light ground (as in the base): not touched.

## Proposed inventory rows

- The Library reader's `.lib-icon-btn` buttons are 36 px and fail the 44 px hit test at 375 (5 fails in the top bar, base and after).
- At 320 the reader's top bar is 340 px wide (Reader settings 20 px past the edge) with the picker at its 96 px floor.
- Under `sideroom` with a white or sepia reader background the end card's text is light on light.

No threat-model rows. No doc states these rules; nothing outside my files needs a sentence.

## PROCESSES

Stack stopped by Windows PID tree (server 46104, Vite 23744). The `Get-CimInstance` listing for `*p104-reader*` shows only the harness's own bash shells for this session's tool calls (nothing of mine running). `netstat -ano | grep LISTENING | grep -E ":(8804|5804|9504) "`: no line. Database `sr_p104_reader_ui` dropped (DROP DATABASE printed). `p104-reader-bin` and my scratch folder deleted. `heavy.sh status` at 14:28: `loungeshots 14:27`, no hold in my name. The stash list was not touched.

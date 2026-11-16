# Wave 80 handoff: library (SEC-AUD-317, 318; D-819, D-820, D-829)

T = 07:30 on 2026-11-16 (no new item from 09:30, checkpoint 09:50, finished 10:15, hard limit 10:30). Clock times (`date`): build lock 07:28 to 07:37; shots round 1 lock 07:59 to 08:09; round 2 lock 08:45 to about 08:50 (the wait for the lock was the long part: tools, dark, bars, developers, voice went first).

Branch `fix/p80-library`, base `c8982549`. Web only plus the one build: no Rust, no migration, no generated file, no i18n fragment, no `.tsx`. Commits `fb51c377`, `7fe1dec9` (SEC-AUD-318) and the handoff commits (SEC-AUD-317).

## Item 1, SEC-AUD-317 (D-819): done
- Confirmed first: `git log 8bc63455..HEAD -- crates` lists 8 commits (SEC-AUD-289/290, 292, 309 and wave 72's items).
- `cargo build -j 4 -p sr-server -p sr-cli --offline` (dev profile, own target `p80-library-target`, `CARGO_PROFILE_DEV_DEBUG=0`): 07:28 to 07:37, 9 min 14 s, from `c8982549`, no warnings, no download. `sr-server.exe` and `sr-cli.exe` copied to `p80-base-bin/`, `BUILT_FROM.txt` written last. Target folder deleted. Lock held 9 minutes (limit 40).
- Result: my stack ran from a copy (`p80-library-bin`, deleted at the end); migrations ran on an empty DB, `/readyz` answered `{"checks":{"database":"ok","storage":"ok"},"status":"ok"}`, and the log line is `listening` on 127.0.0.1:8581 (this server prints no version line; the commit is in `BUILT_FROM.txt`). The CLI (`bootstrap`, `migrate`, `staff list`) and the Library Admin routes worked from this build.

## Item 2, SEC-AUD-318 (D-820, D-829): done, with findings
### Base case (run: yes)
The spec with `library.css` held back (`git show c8982549:apps/web/src/library/library.css` over the file; the after-version kept in scratch and restored), `SR_P80_BASE=1`; base shots in `visual-sweep-shots/p80-library-base` (28; I opened the PDF, settings and EPUB ones). A probe `<span style="color: var(--ui-danger)">` (plus success, warning, info on the background, border and outline) appended in the reader's top bar and in the bottom bar:
- light Library (`sectionThemes.library = 'lounge-light'`): base `rgb(209, 45, 53)` for danger, success `rgb(26, 127, 69)`, warning `rgb(169, 107, 0)`, info `rgb(0, 104, 201)` (the Lounge's colours); after: `rgb(180, 80, 63)`, `rgb(140, 154, 106)`, `rgb(194, 176, 143)`, `rgb(154, 168, 160)` (the Side Room values). The spec asserts the after values and that the probe is gone before the shot.
- shipped (`sideroom`): base and after are the same Side Room values (nothing computed changes).
- The bottom bar gives the same figures.

### Tokens added (reader's-bars dark block of `library.css`)
`--ui-danger: #b4503f; --ui-danger-ink: #fff6f2; --ui-danger-text: #d97a68; --ui-success: #8c9a6a; --ui-warning: #c2b08f; --ui-info: #9aa8a0;` and, from the comparison with the `sideroom` set, `--ui-focus: #d9c9a4` (the kit's Button, Controls and Input read it for their focus ring). Not added because no kit part on the bars reads them: `--ui-bg`, `--ui-bg-overlay`, `--ui-backdrop`, `--ui-shadow-overlay`, `--ui-status-*`. No radius, no font.

### Data and the pages reached
The spec makes, through the API: a series flagged `age_restricted` with an images chapter (3 pages); a book series with a published PDF chapter and a published EPUB chapter; 3 comments on the chapter and 2 on the series from a second account; 2 reviews on the series (the second account's and the Owner's: one account has one review per series). Both accounts confirm 18+ (`/sr/age-gate`). The Owner is already staff, the Admin routes work with `SR_ADMIN_REQUIRE_REAUTH=0`. (Registering the second account on the Owner's request context logs it in as the second account: the spec registers on a separate context.)
**A real upload through the Library's upload route is not possible on the UI stack:** `storage.endpoint = memory:` has no working presigned PUT (`app.rs`: "presigned URLs do not work"); the live Garage job covers uploads. So the PDF and EPUB chapters are real, published chapters (Admin API, `source_key` set) whose manifest `source_url` is replaced in a `page.route` by files the spec writes itself: a two-page PDF as a string and an EPUB 3 as a stored zip (a crc table; no download, no dependency). pdf.js and foliate render them for real. A true upload test needs the live stack. The images chapter's page images are routed to a generated PNG.

### Per page (shipped and light, 1280x800 and 375x812). `scrollWidth <= clientWidth` on every page in both themes (the spec asserts it softly; all pass)
| page | figures | whole already | fixed | listed |
|---|---|---|---|---|
| 18+ badge on a card | text contrast, base: 4.04 shipped (phone and desktop), 4.14 light; after: 5.10 shipped, 4.78 light | position, size | `.lib-age-badge` text from `--ui-danger` to `--ui-danger-text` (#d97a68); the card scrim from 0.70 to 0.82 black | |
| 18+ badge on the series page | base: 3.59 shipped, 4.57 light; after: 5.99 shipped, 4.61 light | | the same rule | |
| comments (series tab) | min text contrast 7.27 shipped, 4.61 light; a 95-character unbroken word wraps inside its card | all | nothing | |
| reviews (series tab, a spoiler review and a rated one) | min 7.27 / 4.61 | all | nothing | |
| reader's bars | probe above; text 16.0:1 | | the status and focus tokens | the page counter reads "3 / 1" for page 1 of 3 in a right-to-left reader (the digits reverse under `direction: rtl`): `Reader.tsx` `pageOfTotal`, not mine |
| reader's comments panel (chapter) | min 6.8 shipped, 4.61 light; the unbroken word wraps; the drawer ends at x 0 (the first measurement caught it mid-slide: the spec now waits) | all | nothing | |
| reader settings dialog | min 6.8 / 4.61; box 421 px wide on desktop, 375 on phone, no inner overflow (374 of 374) | all; it is a kit drawer in the section's theme (blurple accent under light is the light theme's own) | nothing | |
| PDF reader | contrast 16.9; "Page 1 of 2" to "Page 2 of 2" | the bar and the canvas | nothing | on the last page the hovered "Next" shows the light theme's hover grey on the black bar (`.lib-btn:hover`); left |
| EPUB reader | page turn worked ("The lamp 11%" to "22%" on the phone; "The margins 67%" on desktop) | the book and the footer | `.lib-epub .lib-btn { color: inherit; border-color: color-mix(in srgb, currentColor 35%, transparent) }` and its hover. Before, under the shipped theme, the buttons (toc, settings, arrows) were the room's cream `#ece6d6` on the book's white: about 1.2:1, the icons nearly invisible (round-1 shots `epub-reader-*-shipped`). After: `rgb(26, 26, 26)` on `rgb(255, 255, 255)` = 16.7:1 (logged `epub-btn-*`, shots re-taken and opened) | |
| series page tab strip, phone | the strip scrolls inside itself (buttons at x 379 and 497 shipped; 393 and 513 light); the document does not | | | kit Tabs (not mine) |
| "1 chapters" heading | | | | not pluralised (i18n, not mine) |

### Click-through (light, phone)
Posted a comment on the series page (listed as "owner just now", with Edit and Delete); turned a page in the PDF reader and in the EPUB reader; switched a reader setting (the last switch of the images reader's dialog, off to on). **Not done:** setting the EPUB reader's theme to Dark (the spec looks for the text "Dark" in the dialog: timed out; the control is a kit segmented group: a role-based locator is needed).

## Shots
Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p80-library`: 46 PNG (2 themes x 2 sizes: card with 18+, series page with 18+, reviews, comments, reader bars, reader comments panel, reader settings dialog, PDF reader, EPUB reader, EPUB settings = 40; plus light-only click shots: comment posted, PDF page 2 x2, EPUB next x2, EPUB settings after the failed click = 6) and `measure-after.json`. I opened all 46 of round 1 and then, after the EPUB rule and the scroll-to-panel change, the 7 that changed in round 2 (EPUB reader shipped x2, comments x2, reviews x2, EPUB phone); the other 39 of round 2 are the same pages from the same code as the round-1 ones I opened (I did not open them again). `visual-sweep-shots/p80-library-base`: the base shots. `data-theme` read back from `.lib-root` before each shot.
Round 1's reviews and comments shots showed the page head, not the lists (the lists were below the fold): the spec now scrolls the panel into view; those are the ones re-opened.

## Imports (no web build)
`library/library.css` is imported only by `LibraryRoot.tsx`, a `lazyNamed` import in `library/routes.tsx` (confirmed in wave 79, unchanged): the Library's lazy chunk. No web build, bundle, entrance or staff check.

## Tests run
`node scripts/check-css-tokens.mjs` clean; prettier clean on both files; e2e `tsc` clean; `node scripts/i18n-audit.mjs --max 0` TOTAL 0; `node scripts/check-doc-links.mjs` ok; the shots spec: the base run (probe and badge figures above; the spec's click-through stops there), two runs after (the last `1 passed (1.9m)`).

## Docs
Searched for the badge, the EPUB reader and the reader's bars: only historical handoffs. Nothing to update. One sentence for `docs/10-design-system-and-theming.md` (D-820): "A surface kept dark states every `--ui-*` colour token the kit's parts read (the status colours and the focus ring included), so that a part on it never takes a light theme's value."

## Proposed inventory rows
- SEC-AUD-317: "The UI test binaries are rebuilt once per wave whose base has Rust that `p<NN>-base-bin` lacks (D-819): `p80-base-bin` from `c8982549`, 9m14s." Closed.
- SEC-AUD-318: "The Library reader's bars restate every status colour and the focus ring in their dark block (D-820); the 18+ badge reads `--ui-danger-text` on a darker card scrim (4.04 to 5.10 on a card, 3.59 to 5.99 on the series page, shipped); the EPUB reader's buttons take the book's colours (1.2:1 to 16.7:1 under the shipped theme); the Library's unseen pages were looked at (D-829)." Closed. Open items (not token rules): the right-to-left page counter, the PDF last-page hover, the EPUB theme control in the click-through, a true upload test (live stack).
- Threat-model rows: none.

## Changelog sentence
The Library reader's bars keep the Side Room status colours in a light Library, the 18+ badge is easier to read, and the EPUB reader's buttons are visible on its white page in the shipped theme.

## Owner questions
- The right-to-left page counter on the reader's bars reads "3 / 1" for page 1 of 3: isolate the numbers (`<bdi>` or `dir="ltr"`)? (Default: yes, a small `.tsx` change in a later wave.)
- A real PDF and EPUB upload test needs the live (Garage) stack: add one to the live CI job? (Default: leave.)

## Housekeeping
Both stacks stopped by PID (sr-server 38296 and 8292, vite 2768 and 26116), lock released, `sr_p80_library_ui` dropped, `p80-library-bin` and `p80-library-target` deleted, `p80-base-bin` left in place. No stash touched. Untracked scratch only in `p80-library/.scratch` (never committed).

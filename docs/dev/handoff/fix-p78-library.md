# Wave 78, library builder: SEC-AUD-303 (D-802), the Library whole under any section theme

## Done
- Commit 1 (`SEC-AUD-303`): one block `.lib-root:not([data-theme^='sideroom'])` in `apps/web/src/library/library.css` sets `--sr-bg-0/1/2/olive`, `--sr-line`, `--sr-text/-2/-3`, `--sr-gold`, `--sr-gold-ink` from the theme's `--ui-*` (gold = `--text-link`, falling back to `--ui-accent`), exactly as D-802 says. Every name exists in all five Lounge sets (`--ui-text-faint` and `--text-link` come through the `loungeDark` spread). Fonts, radius and sizes are not mapped.
- Filled gold: the block also sets `--lib-fill` = `--ui-accent` and `--lib-fill-ink` = `--ui-accent-ink`; `.lib-btn-gold` (three rules) and `.lib-bell-badge` use `var(--lib-fill, <old value>)`, so nothing changes under a sideroom theme. Chosen because the link colour with white ink fails on Lounge Dark (#00a8fc, 2.7:1); accent #5865f2 with white is 4.61:1 (measured); Editorial keeps its gold with dark ink.
- Commit 2: text over pictures and the reader. (a) The kit's PosterCard corner tag ("#1 TRENDING", "n unread") reads `--sr-text` on a fixed black 70% scrim: under a light theme it was dark on black; `[class*='_tag_']` states `#ece6d6` (the CSS-module class is `_tag_<hash>`; `packages/**` is fenced). (b) The card age badge states `#ff9a85`. (c) The search list's shadow follows `--ui-shadow-overlay`. (d) The reader's top and bottom bars and the sync notice are dark under every theme; their kit parts (the "Page" label, the slider) read `--ui-text*` and were dark grey on near-black in the light theme (seen in the first run); the block restates the Side Room values there.
- Left alone on purpose: the reader's reading themes and its own literals (`.lib-reader*`, `.lib-chapter-select`, `.lib-scrub-thumb`, `.lib-dim`, `.lib-sync-notice`), `.lib-pdf-page canvas` (a white PDF page), the spotlight mask `#000` (an alpha mask).
- Spec `e2e/tests/p78-library-shots.spec.ts` (new; local only: `SR_P78_LIBRARY=1`, `SR_P78_BASE=1` for the base case, `SR_P78_TAIL=1` for the other themes).
- No `.tsx` changed. No Library component sets a colour inline (only `--i` fan angle, a bar width, layout in Submissions).

## Base check (run, with the base CSS held back; the block restored afterwards)
`SR_P78_BASE=1`, my stack (binaries copied from `p72-base-bin`, started and migrated fine), `sectionThemes.library = 'lounge-light'`, `data-theme` read back as `lounge-light` on `.lib-root`:
- desktop: `.lib-side` background `rgb(22, 22, 19)`, `.lib-sectionbar-brand` colour `rgb(236, 230, 214)`; phone: `.lib-bottomnav` `rgb(22, 22, 19)`. All as the plan said (asserted present; the test was green on the base).
- Contrast on the base: brand 1.25, hero heading 1.25, genre chip 1.43, phone bottom link 1.43 (the "Follow your curiosity" heading was 12.6: page text was fine).

## After (same spec, light): computed colours and contrast
| element | base | after |
|---|---|---|
| `.lib-side` bg (desktop) | rgb(22,22,19) | rgb(242,243,245) |
| `.lib-bottomnav` bg (phone) | rgb(22,22,19) | rgb(242,243,245) |
| `.lib-sectionbar-brand` colour | rgb(236,230,214) | rgb(49,51,56) |
| contrast: brand / hero heading / curiosity heading | 1.25 / 1.25 / 12.6 | 12.64 / 12.64 / 12.64 |
| genre chip / sidebar link / phone bottom link | 1.43 / 7.27 / 1.43 | 11.39 / 5.82 / 11.39 |
| Browse kind chip / sort select | black blocks (rgb(28,29,24)) | 10.49 / 11.39 on rgb(232,234,237) |
| `.lib-btn-gold` | not asserted | 4.61 |

Shipped (`sideroom`, set explicitly, read back): computed `background-color` and `color` of `.lib-root`, `.lib-side`/`.lib-bottomnav`, `.lib-sectionbar-brand`, first nav link, first genre chip, `.lib-sidebar`/`.lib-topbar`, desktop and phone: equal to the base (the spec compares with `computed-base-shipped-*.json`; asserted, passed).
Contrast figures are text against the nearest opaque backgrounds (images and gradients ignored).

## Shots (folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p78-library`; the base run's shots are in `base/`)
51 PNGs (the 45 base ones are separate), every one opened by me: per theme (shipped, light) and size (desktop, phone): home, home scrolled to "Follow your curiosity", Browse, Browse with a kind chip pressed, the Filters panel, a series page, Rankings, My library, Latest updates, a submission page (10 each: 40), the phone drawer x2, the reader in light (desktop, phone), Lounge Dark as the section theme (desktop), and the tail: home and Browse in Lounge Ash, Onyx and Editorial (6). Lists and the series come from `page.route` fakes (wave 67's fake series and chapter, plus a six-series list and updates); Rankings and My library are empty states. Light: every text readable; shipped: matches the base computed colours and looks as wave 77's. Lounge Dark, Ash, Onyx, Editorial: one palette each now.
Click-through in light (spec, passed): every sidebar link (desktop) and the four bottom links (phone) with `aria-current` following; Filters panel opened and closed with Escape; a kind chip pressed and released; the phone drawer opened and closed. Portals (the Filters drawer, the phone menu) inherit the block and read whole in light.
Not shot: comments and reviews with real data, lists pages, PDF and EPUB readers (no file), a restricted (18+) card (so the age-badge colour was not looked at), the reader's settings dialog.

## Findings
- Reader in light: keeps its own black page; before commit 2 its page label was dark on black, now readable. Its settings dialog is the kit's and follows the light theme.
- Importers: only `LibraryRoot.tsx:9` imports `library.css` (lazy chunk). No web build run, no first-paint file touched.
- Other CSS that reads `--sr-*` and would need the same block (not read): `activities.css`, `developers.css`, `pwa.css`, `tools/utl-dev.css`, and the Cinema files (the cinema builder). `shelves.css` has one use.
- Lounge Dark: gold text (`--text-link` #00a8fc) on the hovered nav row is about 4.5:1, borderline; not measured.

## Tests run
Spec with `--project=chromium --reporter=line`: base run passed (faults present); after run (with tail) passed twice (before and after commit 2). e2e `tsc` clean, prettier clean on the two files, `check-css-tokens` clean, `i18n-audit --max 0` TOTAL 0, `check-doc-links` ok. No vitest added (computed colours need a browser); no Library vitest asserts a colour.

## Docs
Nothing in `docs/` states the old behaviour. Proposed sentence for `docs/10-design-system-and-theming.md` (not mine), after the per-section themes paragraph: "A section that styles itself with `--sr-*` tokens (the Library, from wave 78) maps them from the chosen theme's `--ui-*` tokens under any theme that is not a Side Room theme, so a light or Lounge theme draws the whole room in one palette; the Cinema does the same, keeping its player, TV screen and text over pictures dark."

## Proposed inventory row
- [x] **SEC-AUD-303** [FE] The Library is drawn whole under any section theme (D-802): under a non-sideroom theme `library.css` maps `--sr-*` from `--ui-*` (one block on `.lib-root`), filled gold uses the theme's accent and ink, text over covers and the reader's bars keep dark values. Spec p78-library-shots.spec.ts (base: near-black sidebar and bottom navigation, brand contrast 1.25 on a light Library).

Threat-model rows: none.

## Changelog sentence
The Library now looks right in any theme you choose for it under Per-section themes, including a light one.

## Owner questions
- Filled gold buttons use the theme's accent (blurple on Lounge themes) rather than the link colour. Fine?
- Gold links take `--text-link` under Lounge themes (Discord blue on Lounge Dark and Ash); Editorial keeps its gold. Keep?

## Cleanup
Stack stopped by PID (API 9584, Vite 33356), lock released at 13:14, `sr_p78_library_ui` dropped, `p78-library-bin` deleted. No stash touched.

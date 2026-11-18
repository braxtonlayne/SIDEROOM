# Wave 100, rtl (D-953): Compare levels and the Invoices shade, right-to-left

**The app has no RTL mode: these are shots of the layout under a forced `dir`, a record of how ready it is, not of a state a member can reach.** Nothing sets `dir` on the root (`i18n/runtime.ts:102` sets only `lang`; the app's CSS is written for it (`:dir(rtl)` rules in cinema, developers, player, tools and server-settings, one `[dir='rtl']` in `emu.css:42`) but a `git grep` of `dir=`, `.dir =` and `'rtl'` in `apps/web/src` finds no code that sets the direction). The spec sets `document.documentElement.dir = 'rtl'` after the page has loaded. Every finding below is therefore **info**, not a row. No app file was changed.

Times (2026-11-18, local): spec written 04:57; first round 04:58 to 05:02 (lock `rtl` from 04:58); one spec error found in my own shots (the LTR hit test opened the phone's "Sections" list and it stayed open for the RTL shot), fixed, second round 05:14 to 05:16. Both rounds on a fresh database and a freshly seeded sweep world, with `SR_PAYMENTS_RATE_SOURCES` (the public coinbase URL of `p98-cue-shots.spec.ts`'s header), `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9`, `SR_ADMIN_REQUIRE_REAUTH=0`. Result of the last run: 1 passed, `findings []`, `specErrors []`, no `FAIL` in the world seed. Theme: Lounge Light, set by `PATCH /users/@me/settings` and localStorage, read back (`lounge-light`) and `data-theme` read in every page.

## Result in one line
Compare levels (1024 and 375) and the Invoices shade (1024) mirror correctly under a forced `dir=rtl`: no box off by more than 1 px, no cut text, no sideways page scroll, same hit-test rows and statuses, no overlap, shade on the edge with more table and none on the other, at the start and at the end. Two bidi-text observations (info, below).

## Compare levels (Server settings > Server Support, owner of the seeded "The Back Booth"), LTR beside RTL
37 boxes measured per state (caption, every `th`/`td`, the card's heading and controls); distance from the inline-start edge of the table (cells) or card (the rest), width and top compared; "not mirrored" lists any beyond 1 px.

| | 1024 x 768 LTR | 1024 RTL | 375 x 812 touch LTR | 375 RTL |
|---|---|---|---|---|
| table | `display:table`, no scroll (over 0) | same | `display:block`, `overflow-x:auto`, over 102 px | same (102) |
| boxes not mirrored (start, end) | | 0, 0 | | 0, 0 |
| text differs (textContent, 37 strings) | | 0 | | 0 |
| cut (`scrollWidth` > `clientWidth` + 1) | 0 | 0 | 0 | 0 |
| `documentElement` over `clientWidth` | 0 | 0 | 0 | 0 |
| dialog's own scroller over | 0 | 0 | 0 | 0 |
| overlaps (> 1 px2), start | 0 | 0 | 0 | 0 |
| overlaps, scrolled to the end | n/a | n/a | 14 | 14 |
| hit test rows (whole panel) | 29: 1 pass, 26 fail, 2 skipped | same rows, statuses, takers | 29: 9 pass, 19 fail, 1 skipped | same |
| sticky label column | static | static | sticky, start 0 | sticky, start 0 (mirrored to the right) |
| `:matches(':dir(rtl)')` on the table | false | true | false | true |
| card inline start / width | 326.7 / 585.3 | 326.7 / 585.3 | 16 / 343 | 16 / 343 |

- **375 shade** (the table's `background-image`, read by pixels: the 16 px strips at each edge with the rule switched off by an injected style and on): LTR at the start, shade at the right edge only; at the end, none. RTL at the start, shade at the **left** edge only (`:dir(rtl)` at `server-settings.css:754` flips it); at the end, none. As the comment says (lines 741 to 760). The sticky label column stays at the inline start (right in RTL) on the card's ground; the figures slide under it.
- The 14 overlaps at the scrolled end at 375 are the sticky label cells lying over the cells that have scrolled under them, by design (same count in both directions).
- The hit test: the same 29 rows, same statuses and the same takers in both directions at both widths (parity is the finding). Most "fail" rows are the sidebar's nav items taking each other's +-21 px points (the 21 px reach of D-856 on 34 px rows; at 1024 a mouse page, at 375 the Sections list's items): there before RTL, in both directions alike; not RTL, not Compare levels.
- **Physical `left`/`right` where a logical property is needed:** none in the sheets that draw these two screens. The only physical directions are the gradients and background positions at `server-settings.css:749 to 758` and `donations.css:254 to 262`, which have no logical form; the first is flipped by `:dir(rtl)`, the second is symmetric on purpose (comment at `donations.css:245 to 250`), and measured right.
- Bidi text (info, not a rule fault): in RTL, values built as `"${n} unit"` show with the unit first: "MiB 250", "kbps 384" (`ServerSupport.tsx` `rows[].cell`, the `upload_mib`/`audio_kbps` strings), and sentence-final full stops and a leading date comma move to the other end (".No contributions in the last 30 days", ",11/18/2026"). That is the Unicode bidi algorithm on English text under a forced RTL paragraph; with a real RTL locale the strings are translated but a number+Latin-unit cell would still reorder. A fix would wrap such cells in `<bdi>` or `dir="ltr"`. Proposed row if RTL ever becomes a mode: "Numbers with Latin units (Compare levels, Invoices dates) reorder under `dir=rtl`; isolate them (`<bdi>`)", severity low.
- The kit's chevrons (the phone's "Sections" select, the Admin selects) are on the inline-end side in both directions.

## Invoices wrapper (Admin > Donations > Invoices, 1024 x 768, Lounge Light; four invoices seeded through the API under `p100rtl*` names)
Wrapper: client 585, scroll 633 (48 px of table beyond the edge). "Beyond" = how much table lies out of sight at that edge. Shade by pixels, rule off vs on.

| state | `scrollLeft` | beyond left | beyond right | shade left | shade right |
|---|---|---|---|---|---|
| LTR start | 0 | 0 | 48 | no | yes |
| LTR end | 48 | 48 | 0 | yes | no |
| RTL start | 0 (the right-hand end) | 48 | 0 | **yes** | no |
| RTL end | -48 | 0 | 48 | no | **yes** |

The shade is on the edge with more table and absent on the edge at its end, in both states and both directions. The computed layers in RTL are the same four as in LTR (two `local` covers of `--ui-bg`, rgb(255,255,255) in Lounge Light, at 0% and 100%; two `scroll` shades at 0% and 100%): symmetric, so the rule does not depend on the writing direction, as its comment says. Cells and rows paint no ground. Page scroll 0 in all four. Headers in RTL run right to left (Invoice at the right).
- **Arrow keys in RTL.** The wrapper is wave 99's region: **tables' keyboard stop (D-949) is not in this base**, so the wrapper is not a keyboard stop here. The probe made it focusable inside the page only (`tabindex=0`, removed again): at RTL start (`scrollLeft` 0), ArrowLeft moved it to -40 (towards more table, which is on the left), ArrowRight brought it back to 0. So the browser's own arrow scrolling follows the physical key, and tables' part should add no key handling of its own.

## Shots (6, all opened, in `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p100-rtl/`; the files of the last run, opened after it)
`compare-1024-ltr.png`, `compare-1024-rtl.png`, `compare-375-ltr.png`, `compare-375-rtl.png`, `invoices-1024-rtl-start.png`, `invoices-1024-rtl-end.png`. Figures: `measure.json` (one JSON line per measure), `totals.json`, `seed-log.txt`. What I saw: the whole page mirrors (sidebar to the right, Close ESC to the left, card text right-aligned, switch to the right of its label, amount buttons in reverse order, the Support button left of the field); the RTL phone table has its label column pinned right and the shade at the left edge; the Invoices table is cut at the left at the start (shade there) and at the right at the end (shade there).
One earlier shot of `compare-375-rtl.png` (first round) showed the Sections list open: my own spec had opened it with `openCollapsed` during the LTR hit test. The spec now shuts it again; the shot in the folder is the second round's.

## Checks (after the last edit of the spec; commit below)
- `npx prettier --check tests/p100-rtl-shots.spec.ts` (from `e2e/`): exit 0, 05:17 (the spec was last edited 05:03).
- `npx tsc --noEmit -p tsconfig.json` (from `e2e/`): exit 0, 05:17.
- `node scripts/check-doc-links.mjs`: exit 0, 05:17.
- The spec ran twice on the stack: the second run is the one reported (exit 0).
- Not run: web typecheck, eslint, i18n audit, any build (no app file changed, none required).
- Which tests I watched fail without a change: none (a record; no app change). Base not run.

## Re-shoot command (the commander, after the sweep world is seeded on the same database)
From `e2e/`, with the stack on ports 8785 / 5785 / 9485 (sr-server env as in the brief, plus `SR_ADMIN_REQUIRE_REAUTH=0`, `SR_PAYMENTS_ESPLORA_URL=http://127.0.0.1:9`, `SR_PAYMENTS_RATE_SOURCES='[{"name":"coinbase","url":"https://api.coinbase.com/v2/prices/BTC-USD/spot","pointer":"/data/amount"}]'`):
```
SR_P100_RTL_ON=1 SR_E2E_BASE_URL=http://localhost:5785 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> DATABASE_URL=<db> \
SR_P100_RTL_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p100-rtl npx playwright test tests/p100-rtl-shots.spec.ts \
--project=chromium --workers=1 --retries=0 --reporter=line
```
It skips itself without `SR_P100_RTL_ON=1`; it fails only on a spec error (a screen that did not open, a `not-reached` hit-test row, the theme not read back), never on a finding.

## Docs, inventory, changelog
- Docs: none changed (a record). Nothing in `docs/` states the direction behaviour.
- Existing rows closed: none. **Proposed rows: none** (no RTL mode: info only). Proposed, only if the owner wants RTL: the low-severity bidi row above.
- Threat model: no row.
- Changelog sentence: "A new local spec records how Compare levels and the Invoices scroll shade lay out under a forced right-to-left direction; both mirror correctly (D-953)."
- AGENTS.md: no sentence.

## Owner question (for Fable)
Is right-to-left a mode the app should have (a locale such as Arabic or Hebrew setting `dir` on the root)? Today the app has none, the layout of these two screens is ready for it (logical properties, `:dir(rtl)` flips), but mixed text (numbers with Latin units, trailing punctuation) is not isolated. Default if no answer: no RTL mode; nothing to do.

## Not done
Nothing outstanding. Other screens were not measured (the task named two).

## Cleanup and the lock
Stack stopped by PID tree (sr-server 39056, Vite 47180 and its child), ports 8785 / 5785 / 9485 checked free; database `sr_p100_rtl_ui` dropped; `p100-rtl-bin` deleted (see the last lines); scratch logs in `SIDEROOM_wt/scratch-rtl/` removed. I found no stash entry and touched none. `heavy.sh status` after release: `free` (05:16). Final `status` just before hand-back is in my reply.

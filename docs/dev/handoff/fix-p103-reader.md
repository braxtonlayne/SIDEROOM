# Wave 103 handoff: reader (code builder, pod library; SEC-AUD-462, SEC-AUD-465 item 1; D-972)

## REACH table (committed 2026-11-18, before 12:35)
| file:line | label / id | page and how to open | sheet rule now | theme | raw or kit | fit | found by |
|---|---|---|---|---|---|---|---|
| `apps/web/src/library/reader/Reader.tsx:844` (`ChapterPicker`, in `label.lib-chapter-select`) | `lib-sr-only` span `library.reader.chapterSelect`; no id | `/library/r/<chapterId>/1` (full-screen Reader, outside the section chrome). Renders only when the series has 2+ chapters. | `library.css:1040` `.lib-chapter-select select` (max-width 220px, height 32px, #1c1d18 / #ece6d6 / #2a2b24) | Reader chrome is its own dark bar (literal colours); page theme = Library `sideroom` | raw (becoming kit) | `fit="content"` (it was content-wide in an inline label, capped 220) | `e2e/tests/p102-pages-shots.spec.ts:488,495,508,510` by `.lib-chapter-select select` and `.lib-chapter-select` (xpath parent = the bar). Keep the `lib-chapter-select` label class. |

How to open the bars: the chrome (`data-chrome="on"`) is on at load and after any `poke()` (pointer move, key, tap on the stage), and it hides by a timer: `CHROME_HIDE_MS = 3000` (`Reader.tsx:46`) with no panel open. A click in the middle of the stage (`onStageClick`) toggles it in strip mode; any key press or mouse move on the stage brings it back. A panel open (comments, bookmarks, settings) keeps it on. For a hit test or a shot: poke, then measure within 3 s, or open a panel to pin it. The p102 spec loops ArrowRight/ArrowLeft + a centre click until the picker is visible.

## Chevron colours (read from `packages/ui/src/tokens/themes.ts`, for libshots to measure with `e2e/support/contrast.ts`)
The kit's chevron is drawn in `--ui-text-muted` on the wrapper's `::after`; the bar ground is `#1c1d18`.
- Library shipped theme `sideroom`: `--ui-text-muted` `#a9a493` on `#1c1d18` = about 6.8:1 (passes 3:1).
- `lounge-light` (chosen for the Library): `#5c5e66` on `#1c1d18` = about 2.6:1 (UNDER 3:1). Computed by hand from the token values; libshots must measure it with the helper. If the chevron is under 3:1 in the light shot, the cure is (a): picker back to raw, rule as the base had it.

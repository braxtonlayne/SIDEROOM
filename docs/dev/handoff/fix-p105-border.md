# fix-p105-border (wave 105, tools builder border; SEC-AUD-476, figures only)

## Clock
Started 15:30, spec compiled 15:34, first shoot 15:35, final shoot done about 15:48, report and handoff 15:52. Nothing queued or running at the end.

## Done (commits)
- `SEC-AUD-476 p105-border-shots.spec.ts (measure only)`, then two small fixes (places, names, per-field theme; blur before hover). Only file of the repo touched: `e2e/tests/p105-border-shots.spec.ts` (new). No app or kit file changed, no build job.
- `C:/Users/braxt/Desktop/SIDEROOM_wt/p105-border-report.md`: the table (theme x place) and the plain-word answers. Read it first.

## Not done
- `sideroom-broadcast` as the main theme: the app does not apply it (read back `lounge-editorial`); covered only on the Live TV page, where its tokens equal `sideroom`'s.
- The watch party panel (1.27:1 in wave 103) was not opened; the Settings cards give 1.19 to 1.56 on the same token.
- No kit field exists on Cinema Browse; no toolbar kit TextInput found (the Library search box is not a kit field: 0 px border). The Library sort select stands for "toolbar".
- Why the Create Channel field's hover/focus shows a text-coloured border with no ring: not traced (flagged in the report).
- eslint on the spec: the web package's eslint ignores `e2e/` ("File ignored because outside of base path"), so it is not run on the spec. Prettier and e2e tsc pass.

## JOBS
| id | class | waited | ran | rc |
|---|---|---|---|---|
| 0027-border-shoot | shoot base | 95 s | 148 s | 1 (spec: dialog places; first run, found the Settings page is itself a role=dialog, names, per-field theme) |
| 0041-border-shoot | shoot base | 68 s | 169 s | 0 |
| 0052-border-shoot | shoot base | 132 s | 171 s | 0 (rerun after "blur before hover": same figures as 0041; the dialog readings did not change) |

## Figures against the task's lists
Task has no must-not-change list (measure only). Figures are in the report; raw in `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-border/figures-base.json` (every field: border, grounds, ratios, hover, focus, what-ifs, per-field data-theme).
Ranges over all kit fields: border/outside 1.19 to 1.61; border/field 1.19 to 1.56; field/outside 1.00 to 1.16; hover 4.81 to 7.5 (Settings; dialog differs); ring 4.33 to 11.55; faint what-if 4.81+ outside, 5.10+ field; muted 5.35+ and 5.82+.

## Shots (all opened after the job ended; all `base-` prefixed, 1024 x 768, no sideways scroll reported)
- `base-border-lounge-light-1024.png`: Settings overview scrolled to cards; select edges visible, faint grey line, clearest of all themes; the fieldset next to it has a much stronger edge.
- `base-border-lounge-dark-1024.png`: both selects' edges are seen only as a thin dim line; the card and field grounds are the same, so no fill difference.
- `base-border-lounge-ash-1024.png`: same as dark, line slightly stronger (1.56 on cards).
- `base-border-lounge-onyx-1024.png`: dimmest; field begins and ends only by a very faint line on near black.
- `base-border-lounge-editorial-1024.png`: faint warm line; readable but weak.
- `base-border-sideroom-1024.png`: faint line, a hair lighter fill than the card; weakest in these (1.19 on cards).
- `base-border-dialog-lounge-light-1024.png`: Channel name field has a light-grey fill on white and a faint border; the field is visible by its tint.
- `base-border-dialog-lounge-dark/ash/onyx/editorial-1024.png`: field fill equals the dialog ground; only the thin border shows the field, the radio rows above are clearer (their own fill).
- `base-border-dialog-sideroom-1024.png`: field is a slightly lighter box with a faint border; the radio rows have no boxes here.
- `base-border-section-shipped-library-browse-1024.png`: sort select seen by a faint border and a slightly lighter fill; the search box is a line only (not a kit field).
- `base-border-section-shipped-live-tv-1024.png`: reminder select, faint border on near black; chips beside it look the same.
- `base-border-section-lounge-light-library-browse-1024.png`: sort select clear light-grey box with a border.
- `base-border-section-lounge-light-live-tv-1024.png`: reminder select clear.
- `base-border-section-shipped-cinema-1024.png` and `...lounge-light-cinema...`: no field on the page (hero, rails, arrow buttons).
Stale shots of the dropped `sideroom-broadcast` main-theme runs were removed by literal path.

## Who imports my files
Nobody: the spec is a test file; no app file touched.

## Checks
- prettier --check on the spec: pass; e2e `npx tsc --noEmit -p tsconfig.json`: exit 0 (15:50). eslint: not run (ignored by config). tsc/vitest/build jobs: not needed (no app file).

## Closing text for SEC-AUD-476 / proposed rows
- Row text: field boundary measured: border 1.19 to 1.61:1 against the ground it sits on in every shipped theme; ground difference 1.00 to 1.16; reaches 3:1 only on hover (faint) and focus (ring). `--ui-text-faint` or `--ui-text-muted` as the resting border clears 3:1 everywhere (4.81+); decision is the planner's.
- Proposed row: Create Channel dialog's Channel name field shows a text-coloured border and no ring on hover/focus (kit Input gives a faint hover and 2 px ring on Settings); cause untraced; taker unknown (Modal or lounge sheet).
- Proposed row: `sideroom-broadcast` cannot be chosen as the account's main theme (the API takes it, the app reads back `lounge-editorial`); expected or not is the owner's call.

## Changelog sentence
Measured the kit field border in every theme (figures only, no change).

## Owner questions (to Fable; Discord-like default first)
1. Field edge: Discord-like default would be to find fields by a ground step, but the kit's ground step is 1.00 to 1.16:1, so the edge must come from the border; default: border takes `--ui-text-faint` at rest and `--ui-text-muted` on hover. Alternative: per-theme in-between values in the report. Does the product want fields at 3:1 at all, given its cards and dividers use the same token at 1.2 to 1.6?

## PROCESSES
`strays.sh p105-border`: strays: none found.

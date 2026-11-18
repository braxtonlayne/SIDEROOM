# Wave 100, select: D-950, a kit `NativeSelect`, used by Server settings Overview; and the wave's one builder web build

2026-11-18. Clock: started 04:54; part, test, adopter and spec written and typechecked by 05:02, commit 6477c39d (05:02); lock `select` 05:02 to 05:05 (one stack round: base and after in the same run, see "Rounds"); web build lock 05:11 to 05:14 (build 05:12 to 05:14); released, `free`. Ports 8782 / 5782 / 9482 free, `sr_p100_select_ui` dropped, `p100-select-bin` deleted. Files: `packages/ui/src/primitives/Select/NativeSelect.tsx` and `.module.css` (new), `packages/ui/src/primitives/native-select.test.tsx` (new), `packages/ui/src/index.ts` (one line), `apps/web/src/settings/server/sections/Overview.tsx`, `e2e/tests/p100-select-shots.spec.ts` (new), this handoff. No Rust, no other sheet, no fragment, no new string, no new token.

## The part
`NativeSelect = forwardRef<HTMLSelectElement, NativeSelectProps>`: every native `<select>` prop passes through, `className` goes on the `<select>` (merged), `invalid?: boolean` sets `aria-invalid` and the invalid look (`--ui-danger` border). It renders `<span class=wrap><select/></span>`; the span carries the chevron (a `::after` of two borders on a rotated box, `--ui-text-muted`, at `inset-inline-end`, `pointer-events: none`). With `multiple` or `size > 1` there is no wrapper and no chevron: the bare `<select>` with the kit look. Look: the kit field's own (Input module): `--ui-bg-raised`, `--ui-border`, `--ui-radius`, `--ui-text`, hover border `--ui-text-faint`, focus ring `2px solid var(--ui-focus)` offset 1px on `:focus-visible`, disabled opacity 0.55, `min-block-size: 40px` and 44 px under `@media (pointer: coarse)`. Logical properties only; no portal, no script, no icon file.

## What changed in Overview
The four `<select>` (`ss-system-channel`, `ss-afk-channel`, `ss-afk-timeout`, `ss-widget-channel`) became `<NativeSelect>` with the same id, value, onChange, disabled and children; the import line gained `NativeSelect`. I dropped `className="ss-select"` on all four: the kit part gives the width (100%), height, padding, border, radius and ring itself. **`ss-select` still exists in `server-settings.css` (editor's file, untouched) and is still used by the other server sections and by Admin** (Channels, Accounts, Limits & settings, the platform audit log, ...); nothing in Overview names it now. Its 44 px rule `@media (max-width: 720px)` is replaced here by the kit's `pointer: coarse` rule: a narrow desktop window gets 40 px in Overview (it was 44 under the base's width rule), a phone gets 44 (measured below). If the owner wants the width rule, it is one line in the kit sheet (`@media (max-width: 720px)`, as `Input.module.css` does); I kept the brief's `pointer: coarse`.

## Rounds (one stack round, base and after in the same run, 05:02 to 05:05)
Fresh database, `e2e/.state` removed, sweep world seeded (no `FAIL`). The base was recorded with `Overview.tsx` put back to the base's text (`git show 17861cd6:...`, from a copy in my scratch folder, not a stash), then the edited file was copied back and the spec ran again; `git status` was clean afterwards (the file equals commit 6477c39d). Spec: `SR_P100_SELECT_PHASE=base` then the default (`after`), figures in `figures-base.json` / `figures-after.json`, comparison in `compare.json`.

| | base | after |
|---|---|---|
| 1024 x 768 (Light and Dark), each of the four selects | 526 x 40 | 526 x 40 |
| 375 x 812 touch (Light), each of the four | 309 x 44 | 309 x 44 |
| boxes of the other controls (all three configs) | | moved 0 (`moved: []`) |
| overlaps between a select and another control | none | none |
| hit test at 375 (`openCollapsed` then `hitTestAll` over `main`) | 27 pass, 0 fail, 0 not-reached | 27 pass, 0 fail, 0 not-reached |
| hit test at 1024 (the helper's points are 21 px away, made for a 44 px phone control) | 1 pass, 26 fail | 1 pass, 26 fail: identical to the base, not a fault of this change (the section's controls are 40 px or less there) |
| document scroll width / client width | no sideways scroll | no sideways scroll |

The selects are not narrower and not shorter, nothing below moved (0 px against an allowed 0). By keyboard (1024 Light, the inactive timeout): `:focus-visible` matches after a key; ArrowDown changed 300 to 900 and the section's "Careful, you have unsaved changes!" bar showed; ArrowUp put 300 back (nothing was saved). The ring computed `2px solid rgb(0, 104, 201)`, offset 1px, base and after the same. The spec printed `problems: []` in both phases (base 24.8 s, after 20.5 s). `data-theme` was read back from the page (`lounge-light`, `lounge-dark`) before each shot and the account theme was set by PATCH and localStorage and read back.

## Shots: 8 made, 8 opened (`visual-sweep-shots/p100-select`)
`overview-1024-lounge-light`, `overview-1024-lounge-dark`, `overview-1024-focus-lounge-light`, `overview-375-lounge-light`, and the same four with a `base-` prefix. What I saw: the base shows the browser's own small down-arrow and its own text inset; the after shows the kit chevron at the inline end (muted ink, in Light and Dark), the text 4 px further in (the kit's 12 px padding), the same boxes, borders and ring. The focus shot shows the blue ring round "5 minutes" in Light, in both trees. **Not shot: the ring in Dark** (the brief asks for it seen in Light and Dark; the focus shot is Light only in my list): it is the kit token `--ui-focus` on the same rule as Input's. The 375 shots show the first two selects only ("No invite channel" and "#general" above it); the other two are below the fold at that scroll (their boxes are in `figures-*.json`, 44 px each).

## Tests and checks (after my last code edit, 05:02; the files are unchanged since)
- `packages/ui` `npx vitest run src/primitives/native-select.test.tsx`: 7 tests passed (renders and merges class, forwards the ref, onChange, disabled, invalid, multiple/size without wrapper, axe with a `<label htmlFor>`). I did not run it against a base (there is no base: the part is new).
- **`src/stories/coverage.test.ts` fails for `NativeSelect` ("NativeSelect has no story")**: DS-PRIM-016 demands a story for every exported component, and the stories files are not mine. Fix for the commander (one of): add `NativeSelect` to a story in `packages/ui/src/stories/Primitives.stories.tsx`, e.g. `export const NativeSelectBasic = () => (<NativeSelect aria-label="Letter"><option>Alpha</option><option>Beta</option></NativeSelect>);` (the stories' axe run then covers it), or add it to `EXEMPT` in `coverage.test.ts` with a reason. Until then the kit's unit suite is red on that one test. The other 82 tests of the two files passed.
- Web `tsc --noEmit -p tsconfig.json` exit 0 (05:00, after the Overview edit); `packages/ui` `tsc --noEmit` exit 0; eslint on `Overview.tsx` exit 0 and on the part and its test exit 0; e2e `npx tsc --noEmit -p tsconfig.json` from `e2e/` exit 0 (05:01, after prettier); prettier on all six files unchanged/clean; `check-css-tokens.mjs` exit 0 (73 files clean); `i18n-audit.mjs --max 0` exit 0 (TOTAL 0); `check-doc-links.mjs` ok. A final re-run of the cheap ones after this handoff is in the commit message of the handoff.

## Web build (05:12 to 05:14, tree of commit 6477c39d, one `pnpm build`, log in the scratchpad)
`npx pnpm@10.12.1 --filter @sideroom/web build`: exit 0 (1 min 50 s). `check-bundle-budget.mjs`: exit 0, first paint `app/app-DFBHlasE.js` **218 355 B gzip, room 37 645 B** (before 218 360 B, room 37 640 B: 5 B less, the chunk hash). `check-entrance-isolation.mjs`: exit 0, "every public surface is clean". `check-staff-delivery.mjs`: exit 0. `NativeSelect`'s CSS landed in `app/ServerSettings-DNmz7rzG.css` only (the lazy chunk of Server settings).

## Who imports NativeSelect
Only `Overview.tsx` (`git grep -n NativeSelect -- apps packages e2e` shows the export line, Overview, and my spec's comment). `Overview.tsx` is imported only by `ServerSettings.tsx` (`import { OverviewSection } from './sections/Overview'`), which `routes/settingsServer.tsx` loads with `lazy(() => import('../settings/server/ServerSettings'))`: a lazy chunk, not first paint. The kit's `index.ts` is imported by first paint, but the new export is a part nothing in first paint uses, so it is tree-shaken: the first-paint figure did not rise (above).

## Not done / next
- Not this wave (as the task says): the other 73 files with a raw `<select>` (the base tree has 77; `git grep -c "<select" -- apps/web/src` gives 73 files now), any change to the kit's `Select`, a story (see above: the stories folder does expect one per primitive, and the coverage test enforces it).
- **The next ten files to move, by count of `<select>`**: `settings/server/sections/Members.tsx` 4, `settings/server/sections/ChannelTabs.tsx` 4, `settings/server/sections/BotCommandPermissions.tsx` 4, `library/Browse.tsx` 4, `tools/cards/InviteTool.tsx` 3, `tools/calc/UnitsTool.tsx` 3, `settings/server/sections/Moderation.tsx` 3, `settings/server/sections/CommunityMore.tsx` 3, `settings/server/bridge/BridgeSection.tsx` 3, `lounge/scrapbook/Gallery.tsx` 3 (then `lounge/InviteDialog.tsx`, `library/SeriesSocial.tsx` with 3 each). Each needs its own base and after boxes: the sections use `ss-select` for width, others bring their own classes.

## Commander's command (from `e2e/`, on a stack with the sweep's world: API 8782, Vite 5782, metrics 9482, `SR_ADMIN_REQUIRE_REAUTH=0`, `DATABASE_URL` and `SR_CLI_BIN` set)
```
SR_E2E_BASE_URL=http://localhost:5782 SR_P100_SELECT_ON=1 \
SR_P100_SELECT_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p100-select \
npx playwright test tests/p100-select-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
After the merge there is no base to compare with (the spec then only records and asserts the `after` rules; `SR_P100_SELECT_PHASE=base` is for a tree where Overview still has raw selects). Delete `figures-base.json` first if the folder holds a base from another tree.

## Proposed rows and sentences
- Decision D-950 (the commander adds it): as the plan's row. AGENTS.md sentence: "A short fixed list uses the kit's `NativeSelect` (D-950)."
- Changelog sentence: "The UI kit has a `NativeSelect`, the browser's own select in the kit's field look (tokens, focus ring, a 44 px height on touch, a chevron at the inline end), and Server settings Overview uses it for its four lists."
- Proposed inventory row (to be numbered from 442): "Kit stories: DS-PRIM-016's coverage test requires a story per exported primitive; `NativeSelect` (D-950) needs one in `stories/Primitives.stories.tsx` (or an EXEMPT line)." The commander can close it in the same commit as the merge.
- No threat-model row.

## Owner questions
- On a narrow desktop window (721 px and under, a mouse) the kit's select is 40 px, as the kit's `Select` and `TextInput` are under `pointer: coarse`; the old `ss-select` was 44 px by width. Do you want the width rule too (Discord parity does not care)? Default taken: `pointer: coarse` only, as the task says.

## Cleanup
Stack stopped by PID tree (server 35464, Vite 17008 with its child), ports 8782 / 5782 / 9482 free, `sr_p100_select_ui` dropped, `p100-select-bin` deleted at the end, `apps/web/dist` is untracked and ignored (not staged), no stash touched. `heavy.sh status` outputs: after the stack round `free`; after the build `free`; just before the reply: see the final report.

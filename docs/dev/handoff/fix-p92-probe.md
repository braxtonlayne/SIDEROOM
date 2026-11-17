# Wave 92, probe: SEC-AUD-404 (D-917), the shared hit-test helper, run over every user Settings page; nothing built

T = 11:47 (2026-11-17). Helper and spec written, typechecked and committed by 11:48. Waited for the lock behind lounge, kit and skins; lock taken 12:11; one stack round to 12:22 (seed, spec run 1, two quick spec fixes, final run); stack stopped, `sr_p92_probe_ui` dropped, lock released 12:22. Final spec run: 47 s (case 1: 5 s, case 2: 6 s, case 3, 20 pages: 35 s). No source file changed; two new files plus this handoff.

## The helper `e2e/support/hit-test.ts`
- `openCollapsed(page, root)`: real clicks on every `details:not([open]) > summary` and every `button[aria-expanded=false][aria-controls]` whose target is inside `root`, except menu/listbox/combobox/dialog/tooltip triggers (`aria-haspopup`, such a role); 3 passes at most, two animation frames; returns `[{kind, text}]`.
- `hitTest(page, locator, opts?)`: scroll to the viewport middle, box, then `elementFromPoint` at the centre, 21 px above and below (`opts.sides`: left and right; `opts.wrapper`: a selector; for checkbox/radio/switch inputs the parent counts as wrapper); returns `{box, points[{at,x,y,taker{tag,role,name,path},relation,ok}], pass}`. Throws `ProbeError` for no match, empty box, centre outside the viewport, or a null point.
- `hitTestAll(page, root, opts?)`: every button, link, input (not hidden or file), textarea, select and role button/link/combobox/checkbox/switch/radio under `root`; one row `{index, tag, role, type, name, path, status: pass|fail|not-reached|skipped, reason, result}`. `display: none` up the tree, or disabled, is skipped (counted), except a collapsed button's panel, which is not-reached. A ProbeError is collected, never thrown.
- The three lines a later spec writes:

```ts
const main = page.locator('main[class*="content"]').first();
await openCollapsed(page, main);
const rows = await hitTestAll(page, main); // then fail on any row with status 'fail' or 'not-reached'
```

Also exported: `CONTROL_SELECTOR` and the types. Checks: e2e `tsc` clean, prettier clean, eslint (run from `apps/web` on both files) clean, `node scripts/check-doc-links.mjs` ok. The spec passes: `--project=chromium --reporter=line`, 1 passed (46.9 s).

## Case 1: Profile, the watched error and the watched pass
Closed "Advanced": `hitTestAll` on the card gives 17 controls, **all not-reached, 0 fail**, 8 of them selects; `hitTest` on a select throws `ProbeError`. `openCollapsed` opened one `<details>`. Open: the page has 9 selects, **all 9 pass** (wave 91's figure). The same page has 6 failing non-select controls (table below).

## Case 2: chips, the watched fail (base)
Nine channels `p92-probe-r1` to `r9` (made only if absent; duplicates from a repeated run are removed). The names are longer than wave 91's, so a row holds **two** chips: **5 rows** (2,2,2,2,1), not 3 x 3. **7 of 9 remove buttons fail "below"** (r1 to r7); the taker is the next row's remove button every time (r1 to r3, r2 to r4, r3 to r5, r4 to r6, r5 to r7, r6 to r8, r7 to r9); r8 passes (nothing under it in the next row) and r9 (last row) passes; **all 9 centres and all 9 "above" points pass**. Same fault as SEC-AUD-401; kit fixes it this wave. Asserted only under `SR_P92_PROBE_BASE=1`, printed otherwise.

## Case 3: every user Settings page (375, touch, Lounge Light; theme PATCHed and read back; `data-theme` `lounge-light` and 375/375 scroll on every page; the 20 pages read from the Settings navigation)
| page | tested | opened | skipped | failures |
|---|---|---|---|---|
| My Account | 14 | 0 | 0 | 2 |
| Profiles | 82 | 1 | 0 | 6 |
| Privacy & Safety | 28 | 0 | 0 | 0 |
| Activity Privacy | 28 | 0 | 1 | 0 |
| Authorized Apps | 0 | 0 | 0 | 0 (empty list) |
| Modules | 1 | 0 | 2 | 0 |
| Devices | 2 | 0 | 1 | 0 |
| Connections | 13 | 0 | 0 | 0 |
| Invites | 1 | 0 | 1 | 0 |
| Storage | 1 | 0 | 0 | 0 |
| Support / Patron | 8 | 0 | 10 | 1 |
| Appearance | 38 | 0 | 1 | 0 |
| Accessibility | 18 | 0 | 0 | 0 |
| Text & Images | 15 | 0 | 0 | 0 |
| Notifications | 22 | 0 | 1 | 0 |
| Voice & Video | 30 | 0 | 1 | 2 |
| Keybinds | 30 | 0 | 0 | 0 |
| Language | 7 | 0 | 0 | 0 |
| Streamer Mode | 1 | 0 | 3 | 0 |
| Advanced | 5 | 0 | 0 | 0 |

**Not reached: 0 on every page.** 11 failures on 4 pages, each by "below" (the link also by "above"). Owner file and proposed row per failure (the commander numbers from SEC-AUD-408):
1. **My Account**, the inline "Edit" buttons of Username and Display name (26 x 15), stacked 28 px apart: the point below takes the next row's button. App file `apps/web/src/settings/user/sections/Account.tsx:528,535`. Row: "Account: the inline Edit buttons are 26 x 15 and the point 21 px below takes the next row's button."
2. **Profiles**, "Edit your profile page" link (162 x 20): above and below return the card. App file `settings/user/sections/Profile.tsx:697` (`a.us-link`).
3. **Profiles**, inside the "view as" preview (`div.us-view-as`, `settings/user/profile/PrivacyPanel.tsx:89`): two segmented inputs (132 x 32 and 70 x 32; taker: the preview block; kit `packages/ui/src/primitives/Slider/Slider.module.css` `.segInput`) and three links of the public-page preview (Bitcoin, Ko-fi, Blog; 269 x 42; taker: a `code` line or the next block). The links are part of a preview of the public page: owner question below.
4. **Support / Patron**, "View leaderboards" link (142 x 20): app file `apps/web/src/support/SupportPanel.tsx:398` (card head `actions`).
5. **Voice & Video**, one kit segmented input (74 x 32, "Optimise for") and one kit slider input (309 x 44): the point below returns the next control's input. Files: kit `Slider` (`packages/ui/src/primitives/Slider/`).
Caveat for the segmented-input rows: the input is the 32 px element, the label around it is the visible segment; a second look by the kit builder may find some are not faults.

## Shots (`visual-sweep-shots/p92-probe`, 4 PNGs and `probe.json`; all 4 opened by me after the release)
`3-01-my-account`, `3-02-profiles`, `3-03-support-patron`, `3-04-voice-video` (each `-375-lounge-light.png`); the failing control is in view, not outlined. No shot for a page without a failure. Cases 1 and 2 have figures only.

## The commander's command (from `e2e/`, stack with the sweep world, API 8706 / Vite 5706)
`SR_E2E_BASE_URL=http://localhost:5706 SR_P92_PROBE_ON=1 SR_P92_PROBE_BASE=1 SR_P92_PROBE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p92-probe npx playwright test tests/p92-probe-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (drop `SR_P92_PROBE_BASE=1` after the kit merge; `SR_CLI_BIN` and `DATABASE_URL` as for the other specs, for the global setup).

## Row text, AGENTS.md, changelog
- SEC-AUD-404: "A shared hit-test probe, `e2e/support/hit-test.ts` (wave 92, D-917): it opens closed `<details>` and collapsed cards, scrolls the control to the middle and treats a null, an empty box or an off-screen control as 'not reached' (a probe error, never a fault). Watched: Profile's closed card gives 17 not reached and no fail, open gives all 9 selects passing; the Onboarding chips give 7 of 9 failing 'below' on the base. Run over the 20 user Settings pages at 375: no control unreached, 11 fail on 4 pages (listed from SEC-AUD-408)."
- AGENTS.md section 4: "A spec that hit-tests a touch target imports `openCollapsed` and `hitTestAll` from `e2e/support/hit-test.ts`; a 'not reached' is a probe error, not a fault."
- Changelog: none (a test tool only).

## Owner questions
Are the links inside Profile's "view as" preview (a rendering of the public page) touch targets that must pass D-856? Default: no, exclude the preview from the pass.

## Cleanup
Stack stopped by PID tree (8706 / 5706 / 9406 free), lock released, `sr_p92_probe_ui` dropped, `p92-probe-bin` deleted (`p80-base-bin` untouched). No stash touched.

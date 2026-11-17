# Wave 96, probe: SEC-AUD-434 (D-941), `hitTestAll` takes menu items by an option

Clock: start 15:25 (the commander allowed it before 15:45); helper and spec typechecked 15:26; lock taken 15:26 (free at once); one stack round 15:26 to 15:28 (seed, my spec, `p92-probe-shots`); stack stopped by PID tree, ports 8741/5741/9441 free, `sr_p96_probe_ui` dropped, `p96-probe-bin` deleted, lock released 15:28. No second round.

## Done
- `e2e/support/hit-test.ts`: `git diff --stat` 8 insertions, 1 deletion. Added: `HitOptions.also?: string` (one doc line), `export const MENU_ITEM_SELECTOR = '[role=menuitem],[role=menuitemcheckbox],[role=menuitemradio]'` (one comment line) beside the selector, two lines in the header "Use" block, and the one changed line: `const all = root.locator(opts.also ? \`${CONTROL_SELECTOR},${opts.also}\` : CONTROL_SELECTOR);`. Without `also` the selector is `CONTROL_SELECTOR`, byte for byte as before. One combined selector, so a node in both lists gets one row. `opts` still goes to `hitTest` as before; `hitTest` reads only `sides`, `wrapper`, `reach`, so the extra key does no harm. Disabled (`disabled` or `aria-disabled="true"`) rows are `skipped` by the existing code for any matched node.
- `e2e/tests/p96-probe-shots.spec.ts` (new). No named data created.
- `git grep -n "hitTestAll(" e2e/tests | wc -l`: 25 before and after; no earlier call changed.

## The round (375x812 touch, Lounge Light read back from the account and `data-theme`, sweep world, Owner; the message toolbar's "More" menu on the channel's last message with a toolbar)
1. `hitTestAll(page, menu)`: 0 rows, 0 `menuitem` rows (the gap, watched).
2. `{ also: MENU_ITEM_SELECTOR }`: 14 rows = 14 `[role=menuitem]`, in document order (Add Reaction, Edit Message, Reply, Forward, Open Thread, Copy Text, View Reactions, Speak Message, Mark Unread, Copy Message Link, Bookmark Message, Remind Me, Pin Message, Delete Message); 14 pass, 0 fail, 0 not-reached, 0 skipped; every box 44 px tall.
3. `hitTest` on each item: box and pass equal to case 2, 14 of 14.
4. Menu closed, on `main`: 148 rows with and without the option, same tags, roles, names, statuses and boxes.
Spec: 1 passed (15.7 s) on the first run (no failure was watched; the gap in case 1 is the watched base). `p92-probe-shots.spec.ts` unchanged, by its header's command on my ports: 1 passed (52.8 s), case 3 over the Settings pages 0 fail, 0 not reached.
Checks: prettier --check on both files, e2e `tsc --noEmit` exit 0 at 15:31, `node scripts/check-doc-links.mjs` exit 0, all after my last edit.
**Helper in the last commit = the helper the round ran: yes.** The spec in the last commit is the one the round ran: yes (I tried a `mouse.move(1, 1)` to clear the shot below, could not get a second round while `world` held the lock, and took it out again).

## The shot
`visual-sweep-shots/p96-probe/long-menu-375-light.png` (1 shot, opened) and `probe.json`. It shows the 14 items whole, but the Remind Me submenu is open and covers the lower-left of the Remind Me, Pin and Delete rows: the mouse that hovered the message (`art.hover()`) sits over that item in the emulated page, and a hover opens the submenu. That is the spec's pointer, not a fault; wave 95's shot of the same menu (`p95-menu/long-menu-375-light-after.png`) has no submenu. The hit tests passed with it open (they test the centre points of the rows, which the submenu does not cover). A re-shoot needs `await page.mouse.move(1, 1)` after the menu opens, before the screenshot (untested).

## The three lines a later spec writes for a menu
    const menu = page.getByRole('menu').first();
    const rows = await hitTestAll(page, menu, { also: MENU_ITEM_SELECTOR });
    expect(rows.filter((r) => r.status !== 'pass' && r.status !== 'skipped')).toEqual([]);

## Commander's command (from e2e/, stack with the sweep world, API 8741, Vite 5741)
    SR_E2E_BASE_URL=http://localhost:5741 SR_P96_PROBE_ON=1 SR_P96_PROBE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p96-probe npx playwright test tests/p96-probe-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
(also set `SR_CLI_BIN`, `SR_DATABASE__URL`, `DATABASE_URL` of the stack.)

## Proposed rows
- SEC-AUD-434 (opened and closed): `hitTestAll` selected controls only, so an open kit menu (items are `role="menuitem"`) gave no rows and a spec looped `hitTest` itself (wave 95). The helper takes `{ also: MENU_ITEM_SELECTOR }`; default unchanged. Proved on the 14-item long menu at 375 (`p96-probe-shots.spec.ts`).
- AGENTS.md sentence (section 4, item 8): "For an open menu call `hitTestAll(page, menu, { also: MENU_ITEM_SELECTOR })`; menu items are not buttons and the default selector skips them."
- Changelog: "Test harness only: the shared touch-target probe can test menu items (nothing a member sees)."
- Threat model: none. Owner questions: none.
- Not done / outside my files: nothing. No stash touched.

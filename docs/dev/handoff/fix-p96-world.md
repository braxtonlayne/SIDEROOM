# Wave 96 world: SEC-AUD-435 (D-942)

Clock: started 15:25, specs edited and typechecked by 15:27 (commit `cee122d8`), lock taken 15:28 (free), round 15:28-15:34, released 15:34, handoff 15:37.

## What I read
- Both specs made `Garden Wardens`, `Night Stewards`, `Spare Keepers` and picked by substring `.first()` (menu `:438`, lounge `:398`). Both `makeState` returned any `ids.json` unread.
- The two `dan`s do not collide: `registerUser(request, 'dan')` (`e2e/support/fixtures.ts`) names the account `dan_<stamp>` with its own stamp and email, so each run gets its own member. But both dans hold chips, so the shared `openPopout` ("first row whose popout has role chips") could open the other spec's dan. I made both specs open the popout whose text names their own first role.
- Other pairs: `git grep apiPost e2e/tests/p9*.spec.ts` finds only channel names (`p90-r*`, `p90-measure-*`, `p91-r*`, `p92-r*`, `p92-probe-r*`) and p92/p93 donation rows; each uses its own prefix. No other pair found.
- The menu spec's counts: only `n > 0`, `short(m)` and `bad(m)` over every item (44 px and hit test kept over every item), and the 1280 base-vs-after boxes in one run. Nothing assumed a count or index, so nothing else needed changing.

## What changed
`p95-menu-shots.spec.ts`: `ROLE_NAMES = ['Menu Alders','Menu Birches','Menu Cedars']` used by `makeState`; the pick is the item whose whole text matches `^\s*Menu Cedars\s*$` (the item's text is only the role name), asserted to be exactly one item; `Ids.guild`; `makeState` reads `world.json` first and returns the cache only if `cached.guild === world.guildId`, else makes anew and writes `guild`; `openPopout` takes the popout that holds `Menu Alders`; header comment brought in line.
`p94-lounge-shots.spec.ts`: `Ids.guild` and the same cache rule; header line on `ids.json`; plus (not in the plan, one small guard) `openPopout` takes the popout that holds `Garden Wardens`. Role names, pick, figures and shots unchanged.

## Runs (one fresh world, db `sr_p96_world_ui`, both shots folders without `ids.json` at the start)
1. `p95-menu-shots` pass (1.3 min), then `p94-lounge-shots` pass (1.0 min). Menu's role menu lists 7 items incl. `Menu Cedars` and the lounge's `Spare Keepers` once; lounge's menu lists its `Spare Keepers` once and `Menu Cedars`.
2. Same world, caches present: `p94-lounge-shots` pass, `p95-menu-shots` pass. Role count in the DB (`roles` table, whole database) before pair 2 and after: 18 and 18 (nothing made twice).
Not run: the discard of a cache from another world (needs a second fresh database). The specs in `cee122d8` are the specs the round ran.
Checks: e2e `tsc --noEmit -p tsconfig.json` exit 0 at 15:26 after the last edit; prettier --check clean; `node scripts/check-doc-links.mjs` see final line below.

## Shots
`visual-sweep-shots/p96-world/menu` (5 png) and `.../lounge` (7 png). Opened two: `menu/role-menu-375-light-after.png` (7 items, whole, inside the viewport, Menu Cedars listed) and `lounge/add-role-menu-phone-lounge-light.png` (same 7 items, one Spare Keepers). Neither is crowded or cut; both menus sit inside the popout/viewport. No proposed sheet rows.

## Commander's command (from `e2e/`, fresh world seeded on the same database; also set SR_CLI_BIN, SR_DATABASE__URL, DATABASE_URL, SR_ADMIN_REQUIRE_REAUTH=0 on the stack). The `ids.json` files in `visual-sweep-shots/p96-world/menu` and `/lounge` are left in place on purpose: both specs must discard them (another world) and pass.
```
SR_E2E_BASE_URL=http://localhost:<vite> \
SR_P95_MENU_ON=1 SR_P95_MENU_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p96-world/menu \
npx playwright test tests/p95-menu-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
SR_E2E_BASE_URL=http://localhost:<vite> \
SR_P94_LOUNGE_ON=1 SR_P94_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p96-world/lounge \
npx playwright test tests/p94-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```

## Proposed text
- Row 435: `p95-menu-shots` has its own role names; both caches are tied to the world. Closes after the commander's shared re-shoot passes.
- AGENTS.md: as the plan gives it ("A spec that creates named data in the shared sweep world gives it names no other spec uses, and a cached `ids.json` carries the world's guild id (D-942).") plus: a spec that opens a popout by "first member with chips" opens the one holding its own role.
- Changelog: nothing a member sees: two test specs now use their own role names and discard a cache from another world.
- Owner questions: none.

## Cleanup
Stack stopped by PID tree, ports 8742/5742/9442 free, `sr_p96_world_ui` dropped, `p96-world-bin` deleted (`p92-base-bin`, `p80-base-bin` untouched), lock released (status: free). No stash touched.

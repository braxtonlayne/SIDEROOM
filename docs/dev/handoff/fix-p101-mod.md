# Wave 101, mod: D-958, the 15 raw selects of Moderation, Community, Command permissions and the bridge move to the kit's `NativeSelect`

2026-11-18. Clock: started 06:19 (`date`); code, spec, typecheck, commit 06:26; lock `mod` first round 06:48 to 06:53 (stack, seed, base and after of all seven views: five views passed, two had a spec error, see "Rounds"); lock again 07:01 to 07:03 (the two views, base and after) and 07:05 to 07:06 (moderation's second shot); released, `free`. Ports 8785 / 5785 / 9485 free, `sr_p101_mod_ui` dropped, `p101-mod-bin` deleted. Files: the five of my row (`sections/Moderation.tsx`, `sections/CommunityMore.tsx`, `sections/BotCommandPermissions.tsx`, `bridge/BridgeSection.tsx`, `bridge/PlanView.tsx`), `e2e/tests/p101-mod-shots.spec.ts` (new), this handoff. No Rust, no sheet, no fragment, no new string, no kit file.

## Done (commits on `fix/p101-mod`)
- `c4f48a5c` D-958 mod: all 15 selects became `<NativeSelect>` (same id, value, onChange, children; `className="ss-select"` dropped on all 15; each import line gained `NativeSelect`); the spec.
- `2c4b16ae` a spec fix (the settings layer is itself a `role=dialog`, so the dialog scope is the innermost one); `93c5070f` a second moderation shot (the alert and timeout selects, below the fold of the dialog); the handoff commit follows.

## The 15 selects
Every select was reached on the stack, moved, and held its box. "ss-select dropped" everywhere: `.ss-select` is `width: 100%` and 40/44 px, which the kit part gives itself; nothing in `e2e/` or the area's `__tests__` finds a select by that class (`git grep -n "ss-select" -- e2e apps/web/src/settings/server/__tests__`: only two spec selectors for toolbar inputs in `p84`/`p85` that exclude it, not affected).

| # | file | select | moved | base box 1024 / 375 | after box 1024 / 375 |
|---|---|---|---|---|---|
| 1 | Moderation | `#ss-rule-trigger` | moved | 760x40 / 303x44 | 760x40 / 303x44 |
| 2 | Moderation | `#ss-rule-alert` | moved | 722x40 / 265x44 | 722x40 / 265x44 |
| 3 | Moderation | `#ss-rule-timeout` | moved | 722x40 / 265x44 | 722x40 / 265x44 |
| 4 | CommunityMore (Guide) | `#ss-guide-ch-0` | moved | 522x40 / 305x44 | 522x40 / 305x44 |
| 5 | CommunityMore (Insights) | `#ss-insights-days` | moved | 560x40 / 343x44 | 560x40 / 343x44 |
| 6 | CommunityMore (Directory) | `#ss-listing-category` | moved | 560x40 / 343x44 | 560x40 / 343x44 |
| 7 | BotCommandPermissions | `#cmdperm-target` | moved | 760x40 / 303x44 | 760x40 / 303x44 |
| 8 | BotCommandPermissions | "Rule for Everyone" | moved | 748x40 / 303x44 | 748x40 / 303x44 |
| 9 | BotCommandPermissions | "Rule for All channels" | moved | 748x40 / 303x44 | 748x40 / 303x44 |
| 10 | BotCommandPermissions | "Rule for #general" (the row with a Remove button) | moved | 660.1x40 / 303x44 | 660.1x40 / 303x44 |
| 11 | BotCommandPermissions | `#cmdperm-add` | moved | 760x40 / 303x44 | 760x40 / 303x44 |
| 12 | BridgeSection | Channel here | moved | 521.3x40 / 279x44 | 521.3x40 / 279x44 |
| 13 | BridgeSection | Channel on Discord | moved | 521.3x40 / 279x44 | 521.3x40 / 279x44 |
| 14 | BridgeSection | Direction | moved | 521.3x40 / 279x44 | 521.3x40 / 279x44 |
| 15, 16 | PlanView | Only here, Only on Discord | moved | 521.3x40 / 279x44 | 521.3x40 / 279x44 |

(15 sites in the files: Moderation 3, CommunityMore 3, BotCommandPermissions 4, BridgeSection 3, PlanView 2. The table has 16 lines because the permission rule select is one site rendered once per rule: three rows on screen.) In every view and every config: the compare found no select shorter or narrower, no control moved (`moved: []`), every block that holds a select (`.ss-row`, `.ss-card`, `.ss-form`, `.ss-bridge-add`) the same height as the base (the rows of Command permissions: 61/61, 61/61, 61/61 at 1024 and 97/97, 97/97, 152/152 at 375), no overlap, no sideways scroll (scrollWidth equal to clientWidth: 1024/1024 and 375/375).

**Hit test, base against after, row by row (same control, same status), `openCollapsed` then `hitTestAll` over the dialog or the section:**

| view | 375 touch | 1024 (the helper's points are 21 px away, made for a 44 px control) |
|---|---|---|
| moderation dialog | 19 pass, 1 fail both | 3 pass, 17 fail both |
| community guide / insights / directory | equal to base (see `compare-*.json`) | equal to base |
| command permissions dialog | 8 pass, 0 fail, 1 skipped, both | 0 pass, 8 fail, both |
| bridge | 8 pass, 1 fail, both | 0 pass, 9 fail, both |
| plan (with the map form on the same page) | 22 pass, 2 fail, both | 0 pass, 24 fail, both |

The one 375 fail in the Moderation dialog is the dialog's own Close button (taker: a `div`, the dialog header), there in the base: a kit/Modal matter, not this change. The spec asserts the rows equal in count, key and status (it printed `problems: []` for every view in the `after` run, which compares with the base figures).

**Keyboard (1024 Light, one per file, the value set back, nothing saved):** Moderation trigger 1 to 5 and the form changed (the field list differs), ArrowUp restored 1; Insights range 30 to 90 and a request `days=90` went out, ArrowUp back to 30; Command permissions target (application default) to `/p101-mod-ping` and back; bridge Direction `both` to `to_discord` and back; plan: Link disabled before, ArrowDown on both lists enabled it, ArrowUp on both disabled it again (so `PlanView` reacts as before). Same in the base run.

## Rounds (all on one fresh database `sr_p101_mod_ui`, sweep world seeded, no `FAIL`, `SR_ADMIN_REQUIRE_REAUTH=0`)
- Base: my five files put back to the base's text from copies in the scratchpad (`cp`, never a stash), the spec run with `SR_P101_MOD_PHASE=base`; then `git checkout -- apps/web/src/settings/server` (the committed edited files), the spec run again. `git status` was clean of everything but the spec afterwards.
- The bridge and the installed bot are `page.route` fakes in the browser (as `p61-bridge-shots.spec.ts` does; names start `p101-mod-`; the bridge's status, discord channels, the module's feature flag, a whole-server plan; the integrations list, the commands and the saved permissions with two special rows and one real channel rule). Nothing is written to the server, no cleanup needed. Moderation, Community and Command permissions' other lists are the real API of the seeded guild.
- Round 1 (06:48 to 06:53): the five views (Community x3, bridge, plan) passed in both phases. **Two spec errors, mine**: Moderation and Command permissions took `[role=dialog]` `.first()`, which is the Server settings layer (it is a dialog too), so `openCollapsed` clicked its section toggle and failed. Fixed (innermost dialog), committed. I then, by my own mistake, started a shortened script that dropped and recreated the database (the first round's sessions went with it); I stopped it at once (stack by PID tree, ports free), released the lock, and queued again. Round 2 (07:01 to 07:03: new database, new seed, the two views, base and after): both passed. Round 3 (07:05 to 07:06): moderation only, base and after, for the second shot. The figures of the five views of round 1 are in the same folder (their world was the first database; comparisons were made in that round, base and after on one database).
- Seed text: no `FAIL`.

## Shots: `visual-sweep-shots/p101-mod` (48 PNG, plus `figures-*.json` and `compare-*.json`)
`moderation`, `moderation-actions` (alert and timeout selects), `community-guide`, `community-insights`, `community-directory`, `bot-permissions`, `bridge`, `bridge-plan`, each `-1024-lounge-light`, `-1024-lounge-dark`, `-375-lounge-light`, and the same 24 with a `base-` prefix. **I opened all 24 after-shots and the base 375 twins of every view (9 of the 24 base files: moderation, moderation-actions, bot-permissions, bridge, bridge-plan, guide, insights, directory at 375, and base-bot-permissions at 1024); I did not open the other 15 base 1024 PNGs (the figures compare their boxes).** What I saw: the base shows the browser's own small down-arrow and its own text inset; the after shows the kit chevron at the inline end, muted ink in Light and Dark; boxes, borders, labels and the ring the same; at 375 every select is 44 px and full width; no overlap anywhere. The plan shot shows "Could not load this section / That no longer exists" under "Checks before the plan": that is the real preflight request, unanswered by my faked bridge (base and after alike), not a fault.

## Found, not mine, not fixed (proposed row)
- **Command permissions at 1024 hides the rule names (base and after alike)**: in `.ss-row` the select is `width: 100%` and the name's `.ss-grow` is `flex: 1`, so the name column is squeezed to 0 px: "Everyone", "All channels" and "#general" (and its "Channel" meta) are not readable in the base shot `base-bot-permissions-1024-lounge-light.png`; only the select and the Remove button show. At 375 the row wraps and the names show. Fix is a sheet rule in `server-settings.css` (the select's span keeps 12rem and the name `min-inline-size`), not a prop. Proposed row text: "Server settings > Integrations > Command permissions: at 1024 the rule rows show no name (the select takes the row); give the name a minimum width or the select a basis."
- **`sections/Community.tsx` has a raw `<select>` of its own** (the `ChannelSelect` helper, id passed in) that no builder owns this wave: it stays raw ("not in my row"; it is the Overview and Onboarding tabs of Community).

## Who imports my files (`git grep -n`)
All five are in the lazy chunk of Server settings: `routes/settingsServer.tsx` loads `settings/server/ServerSettings` with `lazy(() => import(...))`; `ServerSettings.tsx` imports `bridge/BridgeSection` (which imports `PlanView`), `sections/Moderation` and `sections/Community` (which imports `CommunityMore`); `sections/Integrations` imports `IntegrationsOverview`, which imports `BotCommandPermissions`. Nothing in first paint or on a public page. I ran no web build (lounge's).

## Checks after my last edit (the spec's last edit 07:04; the five TSX files unchanged since the 06:26 commit)
- Web `tsc --noEmit -p tsconfig.json`: exit 0 (06:25, after the last TSX edit).
- eslint (`pnpm --filter @sideroom/web exec eslint` on the five TSX): exit 0 (06:26).
- Prettier `--check` on the six files: exit 0 (07:06).
- e2e `npx tsc --noEmit -p tsconfig.json`: exit 0 (07:06).
- `node scripts/i18n-audit.mjs --max 0`: TOTAL 0, exit 0 (07:06). `node scripts/check-doc-links.mjs`: ok, exit 0 (07:06).
- Single vitest files that render my sections, unedited, 06:29 to 06:30: `settings/server/__tests__/moderation.test.tsx` 12 passed, `p14communityMore.test.tsx` 25 passed, `p12safety.test.tsx` 7 passed, `bridge/__tests__/bridge.test.tsx` 21 passed, `bridge/__tests__/sync.test.tsx` 17 passed.
- The spec: base run (old text) printed `problems: []` for all seven views and the after run too, with the comparison on. I watched the base fail only through my spec error described above, not through the page. There is no "fail before the change" case (a style move: the assertion is equality with the base).

## Docs
No doc states a fact that changed (`ss-select` still exists and is used by other sections and Admin). Nothing updated.

## Commander's command (from `e2e/`, on a stack with the sweep's world: API 8785, Vite 5785, metrics 9485, `SR_ADMIN_REQUIRE_REAUTH=0`, `DATABASE_URL` and `SR_CLI_BIN` set)
```
SR_E2E_BASE_URL=http://localhost:5785 SR_P101_MOD_ON=1 \
SR_P101_MOD_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-mod \
npx playwright test tests/p101-mod-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
After the merge there is no base (the spec then records and asserts the `after` rules only; `SR_P101_MOD_PHASE=base` is for a tree where the five files still have raw selects; delete `figures-base-*.json` of another tree first).

## Proposed rows, sentences
- Changelog: "The Moderation rule dialog, the Community Guide, Insights and Directory tabs, the bot Command permissions dialog and the Discord bridge (the map form and the whole-server plan) use the kit's `NativeSelect` for their 15 lists."
- Inventory: close nothing; propose the Command permissions row above (numbered from SEC-AUD-448) and "Community.tsx `ChannelSelect` still raw (D-958 next file)".
- No threat-model row. No AGENTS.md sentence beyond D-950's.

## Owner questions
- None new. Default taken (as D-950): a narrow desktop window (721 to ~1000 px, a mouse) now gets the kit's 40 px select where the old `ss-select` was 44 px by width only at 720 px and under; at 1024 both are 40.

## Cleanup and the lock
Stack stopped by PID tree each round (ports 8785 / 5785 / 9485 free, read), `sr_p101_mod_ui` dropped (`pg_database` lists none), `p101-mod-bin` deleted (`p92-base-bin` untouched), no stash touched (`git stash list` empty), nothing of mine running. `heavy.sh status` after the last release: `free`; the final `status` before my reply is in the report.

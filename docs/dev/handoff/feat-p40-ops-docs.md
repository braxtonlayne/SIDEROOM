# Handoff: feat/p40-ops-docs (OPS-DOC-001, docs truth and the owner's packet)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p40-ops-docs`, branch `feat/p40-ops-docs`, base `origin/integration/phase39` (everything through wave 39). Local commits only: nothing pushed, no PR, nothing merged. No migration, no Rust change, no download, no dependency.

## Resume here

Everything in the brief is done and committed (finished at about 13:00 local, well before the 16:45 limit). Nothing is half-finished. Next for the integrator: merge this branch after wave 39 (PR 70), then read "Found while checking" below; the first item matters most.

## Done

| Area | What changed |
|---|---|
| `docs/13-roadmap-and-phases.md` | A gate row "27-40 The feature phase" (status Partly, with three CI run links) and a new section "Waves 27 to 40": a wave table with handoff links, 38 deferred (D-383), 35b the whole-server bridge, 30b entrance isolation, the gates and where each ran, what is left for the owner |
| `docs/inventory/21-feature-phase.md` | 16 boxes ticked, each with the GitHub run that proves it (merge of wave 37: rust.yml 41208735069, e2e.yml 41208735093, wasm.yml 41208735100, web.yml 41208735072, storage.yml 41208735086, prod-smoke.yml 41205318174). The five game-server items are listed under "Deferred, not open" (struck through). A Wave 40 section holds OPS-DEP-001 (open, dated 2026-11-17) and OPS-DOC-001 |
| `docs/15-threat-model.md` | New rows T33 (bridge, selective: token, loops, teardown, kill, erasure), T34 (whole-server sync: permissions master copy, plan, holds, teardown options), T35 (profile tiers, the public route, guestbook), T36 (rings), T37 (emulator: cores, `/emu/cores`, the `/games` CSP, BIOS, co-op seats), T38 (drawer notes, the rate feed, embeds, tool rooms); T30 gained its presign TTL, range and key controls. Every reference was grepped and `threat-model.test.mjs` is green |
| `docs/09` | Rows: profile privacy and pending guestbook posts, tool rooms (7 d), invite cards, jukebox and listening rooms, private notes and loot tables, currency rates, the music library, the emulator, Activities data, embeds. (Rings, bridge, whole-server, scrapbook, mounts and presigns were already there.) |
| `docs/05` | New section 5.3c: the route groups of the feature phase with their AUTHZ classes |
| `docs/07` | New table "Who may do what in the feature phase": `view_games`, co-op seats, bridge owner-only acts, jukebox, profiles, rings, scrapbook, Lock the Doors, module switches (the storage grants paragraph already named `view_games`) |
| `CHANGELOG.md` | "Phase 40" at the top; it notes wave 33 folded into 32 and 38 on hold, which the file did not say |
| Member handbook | Seven new sections (`storage`, `player`, `drawer`, `profiles`, `bridge`, `activities`, `emulator`) and an `operators` section in `apps/web/src/help/Handbook.tsx`; the existing `games` section is the game-servers "coming later" line. New operator page `/help/operators` (`Operators.tsx`, route added in `router.tsx`, two lines). Strings are in the new fragment `i18n/messages/en/p40-ops-docs-1.ts`. `handbook.test.tsx`: section list updated, two new tests (figures agree with doc 09; the operator page lists every switch the registry fixture knows) |
| `deploy/OPERATOR.md` | New block at the end of section 3: BYOS keys, music/jukebox/tools keys, rings, scrapbook, profiles, the bridge (heading "The Discord bridge (D-381, D-386)": settings, how an owner turns it on and off), the emulator, search engines and the front door, and a table of the nine jobs with ids and cadence |
| `docs/legal/lawyer-brief.md` Part E | Checked against the decisions log. Fixed: the "second-factor step" claim now follows P35-BRG-4 (only when the server's level asks); the Q21 addendum gains the teardown options (P35B-Q10), conflict retention (P35B-UI-2) and the Administrator warning; Q23 states the cores actually pinned, the HLE BIOS decision (P37-EMU-2), no ROM shipping (P37-EMU-5), SNES and Genesis absent (P37-EMU-1) and the `extract7z.js` licence. New Q25 (music in voice). Q24 and Q26 stay out (D-383) |
| `docs/dev/owner-checklist.md` | Section 4 is now "Legal and decisions" (4a legal, 4b the decisions of the phase with their current defaults, 4c earlier questions) and section 5 lists every download. All 16 items the brief named are there. Section 3 (GitHub) is ops-ci's and was not touched |

## Tests and gates run

- `check-doc-links.mjs`, `check-ops-files.mjs`, `node --test scripts/threat-model.test.mjs` (5 pass): green on the final tree.
- apps/web: `vitest run src/help src/i18n src/router.test.tsx src/security src/lib src/lounge/__tests__/helpButton.test.tsx --maxWorkers=2` green (handbook tests include the new ones); `tsc --noEmit` clean; eslint on `src/help`, `router.tsx` and the new fragment clean; `prettier --check` on docs, deploy, CHANGELOG, help and router clean; `scripts/i18n-audit.mjs` TOTAL 0.
- Not run (not touched): Rust, OpenAPI, bundle budget (the operator page is small and not a new lazy chunk; the integrator's `check-bundle-budget.mjs` run will say), Playwright (the ports belong to the wave 39 integrator).

## Found while checking (for the commander)

1. **The bridge e2e specs skip themselves on CI.** `bridge.spec.ts` and `bridge-whole-guild.spec.ts` need the fake Discord (`e2e/support/fake-discord.mjs` on 8090) and no workflow starts it, so in e2e run 41208735093 both show as skipped. The bridge is proven by the Rust suites on CI and by the local runs in the wave 35 and 35b handoffs, not by a browser run on GitHub. docs/13 says so. The fix is ops-ci's area: start the fake in the `playwright` shards or the `live` job.
2. Also skipped on CI: `jukebox.spec.ts` (needs the SFU) and `convert.spec.ts` (needs the media stack); `noindex.spec.ts` skips in the ordinary shards but ran and passed in `prod-smoke.yml`. `static/pwa-security.spec.ts` is in no workflow. For these reasons SEC-ENT-002, SEC-ENT-004 (voice soak not rerun), MUS-JB-001, MUS-JB-003, UTL-AUD-001, PRF-PRO-001, PRF-UNF-001, MUS-SKN-002 and OPS-IMM-003 stay unticked.
3. The Phase 40 changelog heading changes the What's new version, so members see the badge once after the upgrade.
4. `integration-phase27` to `29` have no owner section; the owner items of those waves are the checklist's existing rows.

## Decisions I made (defaults, reversible)

- The operator page is a real route (`/help/operators`), which touches `router.tsx` (outside `help/*`, two lines).
- Profiles, rings and the scrapbook get their own handbook section; the existing privacy paragraph is unchanged.
- Rings and the scrapbook are described "where this server has them on" because both default off.

## Open owner questions

All are in `docs/dev/owner-checklist.md` sections 4b and 4c with their defaults; nothing new was added beyond those.

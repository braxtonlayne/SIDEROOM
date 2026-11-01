# 46-A Land.io polish (ACT-LND-002): report

Branch `feat/p46-external-landio`. Everything is inside `packages/starter-packs/activities/landio/**`. No dependencies were added and `pnpm-lock.yaml` is untouched. The commander folds this file into `docs/dev/handoff/feat-p46-external.md` and deletes it.

## Done

| Brief item | What | Proof |
|---|---|---|
| 5.3 Bot backfill | Bots are named `Lumen bot`, `Tofu bot`, then `Lumen bot 2`, `Tofu bot 2`, and so on, taking the lowest free name with no RNG draw (`shared/backfill.js`). Each round mode declares `minHeads` (rounds 2, Last Land / Shrink / KotH 3, Teams / Infection / Draft 4). The activity's arenas set `cfg.backfill`, so a small lobby is topped up to the mode's minimum even when the host set fewer bots. One bot leaves per tick as people sit down, and bots come back when people leave. Every other bot choice still uses the seeded RNG. The HUD shows the "bot" part of the name as the badge, and on phones bot rows read "Tofu 2" in muted italics. | `test/backfill.test.js` (7 tests: names, the minimum for every mode, leave on join and return on leave, a host-set count wins, determinism, the host engine backfilling a Teams lobby) |
| 5.1 Draft Night run | `shared/modes/draft.js`: a **run of 3 rounds**. **Keeper pick**: the first draft of your first life each round stays for the rest of the run and is handed back at every spawn. This is the pick between rounds. **Rarity**: round 1's keeper draft is fair; later drafts lean toward commons for the run leader and toward rare and epic for the trailing half. **Synergy**: three families, Runner (dash, haste, phase, bloodlust, rewind), Trickster (grenade, snare, mirage, emp, wall) and Builder (claimflag, overgrowth, shield, secondwind). Drafting 3 cards of one family in a life (keepers count) turns on its set bonus, and 5 turns on the stronger version: ability recharges 20% / 33% faster; leaving your land gives a 3 s / 5 s ward (Shield's effect); land scores +8% / +15%. **Run points** go 5/3/2 by place, then 1 for everyone else listed. **End-of-run summary**: the last round's results carry `result.run` (standings, points gained, everyone's keepers, the champion), and then a new run starts. Bots draft for rarity, synergy and upgrades. A host restart or a mode change starts a fresh run, and a natural new round carries the run on. New ModeDef hooks: `onPick`, `onRoundOver`, `carry`. HUD: a "Keeper pick" heading with a note, family tags on cards, set chips ("Runner 2/3", then the bonus), a "Round 2/3" chip, and run standings on the results card. | `test/modes/draft-run.test.js` (11 tests). All 11 old `test/modes/draft.test.js` tests still pass unchanged. Tuning tool: `tools/draft-balance.mjs` (numbers below). |
| 5.4 Host migration (game half) | `host/handoff.js`. `successor()`: the longest-connected member, ties broken by member id in plain code-unit order. `successorFromSnapshot()`: every member computes the same answer from the published seat order. `snapshotEngine()`: a compact JSON snapshot (simulation with RLE tile layers; seats in sitting-down order with their input counters and not-yet-applied inputs; room settings). `resumeEngine()`. The `HostHandoff` typedef (`snapshot` / `successor` / `hostChanged`) is implemented by `client/net/sdk-transport.js` as `transport.handoff`. Snapshots are only published when `snapshotEveryTicks` is set (off by default). With them on, a guest waits up to 5 s for a new host before ending. The loopback hub gained `setHost()` so tests can run the whole flow. | `test/handoff.test.js` (7 tests). The key one, "a handoff between two simulated hosts resumes identically", compares the state hash of the old host (control) and the successor every tick for 280 ticks after the handoff. There is also a loopback end-to-end test (successor hosts, the other guest keeps its seat and player and its input reaches the new host, no "host-left") and a Draft Night mid-run snapshot test. |
| 5.2 Guests everywhere | `client/platform/sideroom-platform.js`: with `config.guest` the page plays under `callsign`. It never sends `getInstanceConnectedParticipants`, never subscribes to member events and never calls `setActivity`. `GUEST_COMMANDS` / `GUEST_EVENTS` list the allowed bridge surface (handshake, `dataSend`, `subscribe` for `DATA_MESSAGE` only, storage). The SDK loader is injectable. Touch controls were already there (swipe to turn, tap to stop, tap the card and slot buttons). The tips now talk about swipes and taps on coarse-pointer devices. I checked 375 px by hand in Edge with Playwright: start card, settings, Draft Night play with the keeper pick, set chips, and the results with run standings. | `test/host-guest-sdk.test.js`, 2 new tests. A fake guest bridge throws on `authorize` / `authenticate` / `getChannel` and on every other member command or member subscription. A guest joins, plays and steers through the platform, and every bridge call is checked against the guest list. A second test proves the fake really refuses. |
| 5.5 Polish | See the list below. | Manual 375 px screenshots |
| Task: vitest comment | `vitest.config.mjs` now says Land.io is a workspace package and gives the pnpm command. | |

**Polish changes:**
1. Touch-aware tips: "Swipe to steer, tap to stop", "Pick a card: tap one", and tapping the item and ability slots.
2. The card tip no longer says "You earned cards!", which was wrong in Draft Night.
3. New round win and round end sounds (`roundWin`, `roundEnd`, synthesised). The results screen used to play the land-capture sound.
4. A one-time tip explains set bonuses when the first family card is drafted.
5. Leaderboard on phones: bot rows are short ("Tofu 2"), so five bots no longer all read "Tofu b…".
6. Head labels no longer say "Lumen bot · bot".
7. The phone draft strip sits clear of the minimap.
8. The Draft Night round chip is short enough for a 375 px screen ("4:55 · Round 1/3").
9. Set chips sit under the round chip, not over the inventory.
10. Phone run standings show the top three (and you), so "Next round in…" stays on screen.
11. "1 pt", not "1 pts".

**Balance runs:** `node tools/draft-balance.mjs 40 150` (40 bot-only runs, 120 rounds of 150 s, 10 bots on 80×80). Final numbers:
- Round wins per holder, by the family a bot leaned on: Runner 9.7%, Trickster 7.6%, Builder 13.3%, none 8.6%.
- The share of rare and epic cards in keeper offers is 0.38 for the leading half and 0.58 for the trailing half (the catch-up works).
- Champion points ranged from 7 to 13 (a run is not decided in round 1).

Changes made along the way:
- Set bonuses started at 2 held cards, but every bot had one. They now need 3, and 5 for the stronger version, counting cards drafted rather than cards held.
- Trickster started as "a free item every 30 s", but bots made poor use of the items, so its holders lost ground. It is now the ward, which raised its holders' score share from 0.33 to 0.40.
- Builder went from +10% to +8%.

The `test/modes/draft-run.test.js` balance guard keeps every family under 60% of round wins and checks that runs crown at least 3 different champions.

## Not done

- **Live host migration in SIDE ROOM.** The platform cannot move `hostId` and only the launcher may write `host:*` keys, so `hostChanged` has no caller yet and snapshots are off by default (see the follow-ups).
- **Real-device check.** I checked 375 px in Edge with touch emulation, not on a real phone or as an installed PWA. The e2e `guest-play.spec.ts` was not run (it is outside this folder; the brief says the project's agents run it).
- **Changing what bots do with items.** Bots underuse items, which makes Trickster numbers lower for bots than they will be for people. I left bot item AI alone as out of scope.

## Tests run

All commands were run from the repo root unless noted.

- `npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 --filter @sideroom/activity-landio test`
  - The run after the Draft Night commit was fully green: 37 files, 317 passed, 1 skipped.
  - The final full run gave 38 files, 324 passed, 1 skipped and **2 failed**, both timing tests:
    - `test/modes/draft.test.js > timing: tick stays cheap…` (draft 1.57 ms against classic 0.53 ms per tick)
    - `test/modes/shrink.test.js > timing: the mode costs under 0.3 ms…` (0.75 ms extra). I did not touch Shrink beyond adding `minHeads`.

    The run before it failed only the draft one.
- **That timing failure is environmental, not a regression:**
  1. The unmodified base commit `1734666e`, extracted to a scratch folder and run with the same Vitest, fails the same test (draft 2.15 against 0.72 ms) and the `shrink` timing test (0.51 ms extra), under the same machine load. Another agent was running at the same time.
  2. Outside Vitest, the same measurement gives classic 0.25 ms and draft 0.26–0.29 ms per tick.
  3. Run on its own (`npx vitest run test/modes/draft.test.js -t timing`), the test passes.
  4. The slowdown only appears inside Vitest after earlier tests in the same file have created many small arenas, for every mode including classic, and it is the same on the base commit.

  The commander should re-run the suite on a quiet machine.
- `npx tsc --noEmit -p tsconfig.json` (in landio): clean.
- `node tools/bundle.mjs` (in landio): passes its checks, 93 entries, 567 KB unpacked, 208 KB zipped. `dist/` is not committed.
- New test files: `test/backfill.test.js` (7), `test/modes/draft-run.test.js` (11), `test/handoff.test.js` (7). Two tests were added to `test/host-guest-sdk.test.js`.

## Follow-up list (outside my folder)

1. **Platform host reassignment (D-388).**
   - Files: `crates/sr-activities` (instance host), the client bridge (`apps/web` activity host page) and `packages/embedded-app-sdk/src/{sdk.ts,data.ts,protocol.ts}`.
   - Change: when the host member leaves, pick the successor with the same rule as `host/handoff.js` `successor()` (longest connected, ties by member id), let the successor write `host:*` keys, and deliver a `HOST_CHANGED { host_id, previous }` event.
   - Why: Land.io's `transport.handoff.hostChanged(newHostId, leftId)` resumes the game from the newest snapshot. Once that event exists, call it from `client/platform/sideroom-platform.js` and turn on `snapshotEveryTicks` (suggested 35, every 5 s) in `client/main.js`.
2. **SDK channel host getter.**
   - File: `packages/embedded-app-sdk/src/data.ts`.
   - Change: let `DataChannel.host` change (it is fixed at attach).
   - Why: the same migration as item 1. Land.io's loopback hub already models it with `setHost()`.
3. **Inventory row `ACT-LND-002`** in `docs/inventory/`, the CHANGELOG line, a decisions-log note for Draft Night's run rules and for bot backfill (`minHeads`), and a threat-model note. The snapshot carries seat names and member ids (already visible to every member in game) and no tokens; a successor trusts the host's last snapshot.
4. **`e2e/tests/guest-play.spec.ts`:** add a step that opens Draft Night as a guest and picks a keeper by tap (touch controls through the real bridge).

## Owner questions (decided with a default)

1. **Bot names:** I used `Lumen bot`, `Tofu bot`, `Lumen bot 2`, `Tofu bot 2`, and so on. A person may still name themselves "Lumen bot"; the roster's bot flag still tells them apart. Should person names that match the bot pattern be refused?
2. **Mode minimums (`minHeads`):** rounds 2; Last Land, Shrink and KotH 3; Teams, Infection and Draft Night 4; Classic 0. Backfill is on for the activity's arenas and off in the rules defaults, so tests keep their exact bot counts. Are these the right numbers?
3. **Pick between rounds:** the keeper pick happens at the start of the next round, not on the results screen, because the simulation freezes inputs between rounds. Is that acceptable, or should the results screen take picks?
4. **Run length:** a Draft Night run is 3 rounds of the mode's 5 minutes (the host's round-length setting still applies). Run points are 5/3/2/1. The final round's winner is the run champion, which can differ from that round's own top scorer.
5. **Set bonus numbers:** the thresholds are 3 and 5 cards drafted in a life, keepers included. Runner gives 20% / 33% faster recharge, Trickster a 3 s / 5 s ward with a 20 s recharge, Builder +8% / +15% score. These were tuned against bots only.
6. **Host handoff snapshots** are off by default (`snapshotEveryTicks: 0`) until the platform can move the host. A 100×100 snapshot is under 40 KB of JSON (asserted), sent in 3,000-character chunks. Every 5 s that comes to roughly 4–8 KB/s per member, inside the 32 KB/s budget. Should they be turned on as soon as the SDK event lands?
7. **Guest Rich Presence:** guests set none (`setActivity` is skipped). The participant list for a guest is just itself, and the host's roster carries the names of the people playing.
8. **Restart semantics:** the host's "Restart the round" and any mode, map or size change start a new Draft Night run.

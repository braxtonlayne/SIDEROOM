# feat/p49-lobbies: Active lobbies on the Arcade (ARC-LOB-001, D-483 to D-487, T53)

Builder "lobbies", wave 49. Branch `feat/p49-lobbies` from main `88b87885`. Not pushed. Built to Fable's spec
(`SIDEROOM_wt/p49-lobbies-fable.md`). No migration, no new table, port or gateway event.

## Done

| Commit | What |
|---|---|
| `eac67224` | **Server.** `LobbyView` (`sr-activities/src/model.rs`); `ActivityService::lobbies` (`service.rs`): one SQL read of the running instances in the caller's own `guild_members` guilds (soft-deleted servers left out), newest first, at most `LOBBY_SCAN` = 100, then each through `ChannelAccess::perms` with `view && may_take_part` (a permission error drops the row with a warning, never the list); `sort_lobbies`: own (`joined`) first, then `player_count`, then newest; at most `LOBBY_MAX` = 20. `GET /users/@me/activity-lobbies` (`SessionOnly`; `{enabled:false, items:[]}` when Activities are off); bots refused in `auth::policy::bot_denied`; route matrix row (`authenticated self deny - -`); added to `soft_empty` in the authz matrix. Seven Postgres tests in `crates/sr-activities/tests/db.rs` (the names in the spec, with the voice one spelled out in full) and two HTTP tests in `crates/sr-api/src/routes/activities/tests.rs` (the session-only list and the off answer). |
| `5c028123` | OpenAPI document, the TypeScript client and the MSW handlers regenerated (`mocks:check` green). |
| `179e2564` | **Web.** `Lobby`/`Lobbies` types; `LobbyApi` (`lobbies`, `join`) next to `ActivityApi` (kept separate so the existing fakes of `ActivityApi` did not change); `useLobbies` (fetch on mount, refetch coalesced to one a second on `SR_ACTIVITY_INSTANCE_UPDATE`, a failed fetch keeps the last list); `LobbiesSection` between the hero and Playable now (hidden when empty, failed or off), its `#lobbies` pill only with items; Join (voice: `joinCall` first, false stops with "That call could not be joined."; then `POST .../join`; then the channel); Return (no `join`); 404 "This game has ended." and a refetch after 2.5 s (so the line is read before the card goes); 403 or another error "This game could not be joined." (403 also refetches); "N live" chips (`LiveChip`, `liveCount`) on Playable now and Featured cards, in the meta line; `builtinArtFor`. Strings in `apps/web/src/i18n/messages/en/p49-lobbies-1.ts`. Vitest `apps/web/src/arcade/__tests__/lobbies.test.tsx` (11) and a placement test in `arcadePage.test.tsx`. |
| `1b18e271` | Fix from the visual check: the server and channel line was cut to "#gen…" in a 16 rem card; it now wraps to two lines before an ellipsis and its icon keeps its size (before: `probe/`, after: `probe-r2/`). |
| `8b80f989` | Test typing for the full `tsc` the build runs. |
| (docs commit) | D-483 to D-487 in `docs/01-decisions-log.md`, T53 in `docs/15-threat-model.md`, the route in `docs/specs/developer-platform.md` 3.10, one sentence in `docs/11` section 3, the ARC-LOB-001 row under "Wave 49" in `docs/inventory/21-feature-phase.md` (ticked: the proof ran), the "Not done" line of `feat-arcade-redesign.md` pointed here. |

## Visual check (AGENTS.md section 4)

Own stack (sr-server built from this branch on 8213, Vite 5213, DB `sr_p49_lobbies`, Activities on). Shots in
`SIDEROOM_wt/visual-sweep-shots/p49-lobbies/`, all looked at:

- `sweep/` (Editorial "dark" pass and the Lounge Light pass, where the Arcade is Editorial by D-476) and
  `sweep-loungedark/` (`SR_SWEEP_DARK=lounge-dark`): route `arcade` at 1280x800 and 375x812 with no lobbies. The page
  is unchanged (no section, no pill). Automatic checks: 0 flags in both.
- `probe/` and `probe-r2/` (a scratch Playwright probe with the lobby route mocked, deleted, not committed): 0, 1, 3 and
  20 lobbies at desktop and phone in the Arcade's own Editorial, Lounge Light and Lounge Dark (`sectionThemes.arcade`).
  Measured per shot (`report.json`): one row always, no page overflow, the strip scrolls sideways with the edge fade
  at 20 (desktop) and at 3+ (phone), the body never overlaps the painting, the pill appears with items, no console or
  page errors. `*-strip.png` are close-ups of the section.
- Flows on the real sweep world (`probe-r2/flow*.png`, log in `flows.txt`): a second member started the approved
  "Doodle Board" in the text channel and in the voice channel. Join on the voice lobby: this stack has no SFU, the
  call fails, the card says "That call could not be joined." and **no** `join` request was sent (`flow2`). Join on the
  text lobby: one `join` request, the channel opens with the Activity panel above the messages, "2 people" (`flow3`).
  Back on the Arcade the lobby says Return (`flow4`); Return opens the channel with **no** new `join` (`flow5`). The
  phone strip with real data: `flow6`. Both instances were ended afterwards.

## Gates run

- `cargo fmt --check`; `cargo clippy -j 2 -p sr-activities -p sr-api --all-targets -- -D warnings`: clean.
- `cargo test -j 2 -p sr-activities -- --include-ignored` (db.rs 20 incl. the 7 new, trust_db 9): pass.
  `cargo test -j 2 -p sr-api` (816 lib tests, route_matrix and the other integration tests): pass.
- Authorization matrix with the storage actors (`every_route_enforces_its_authz_row`, `storage_scopes_hold_over_http`):
  both pass (2 passed, 176 s) on this branch.
- OpenAPI regenerated from the branch's `sr-server openapi`; client and MSW handlers regenerated; `mocks:check` green.
- Web: `tsc` (both the `-p tsconfig.json` gate and the build's full `tsc`), eslint and prettier on the changed files,
  vitest `src/arcade` and `src/activities` (17 files, 160+ tests), `check-css-tokens` clean, `i18n-audit` 0.
- `pnpm --filter @sideroom/web build`, `check-entrance-isolation` (every public surface clean),
  `check-bundle-budget` (first paint `app` 246.9 KB, unchanged).
- `playwright test --list --project chromium`: 68 files load.
- `node scripts/threat-model.test.mjs`: **fails on this branch alone**: it wants rows T1..Tn in order, and T51 and
  T52 do not exist here (T52 is the picker builder's; nobody was given T51). See owner questions.

## Not done

- No e2e spec for the strip (the spec asked for a probe, not the sweep; the sweep world has no running lobby).
- The authz matrix ran on this branch only; the integrator runs it again on the merged tree.

## Owner questions (Discord-like defaults taken)

1. **403 at Join.** D-486 lists "That call could not be joined." or "This game has ended."; for a text lobby the first
   reads wrong, so a 403 (or any other failure) says "This game could not be joined." and refetches. Keep?
2. **The ended line stays 2.5 s** before the refetch removes the card (an immediate refetch made the card vanish
   before the line could be read). Keep?
3. **T51 is unassigned** (wave 49 gave T52 to picker, T53 to lobbies); `scripts/threat-model.test.mjs` needs
   consecutive rows, so the integrator must renumber (or someone owns T51) when merging.
4. "N live" chips are on Playable now and Featured cards, not on the two hero cards (the same games sit just below,
   and the hero is hidden on a phone). Add them there too?
5. Lobby times say "just started", "for N min", then "for N hours"; they refresh with each refetch, not on a clock.

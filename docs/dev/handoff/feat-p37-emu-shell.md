# Handoff, wave 37 emu-shell (ACT-EMU-001): the core loader and the shell

Branch `feat/p37-emu-shell` (base `integration/phase37`, origin/main 5b242383). Not pushed. Migration 1800.

## Done

**Server (Rust)**
- `view_games`, the sixth storage grant: `Grant::ViewGames`, `Kind::Games`, `Access.games` in
  `crates/sr-storage/src/scopes.rs`. Every key under `emu/` is kind `games`, whatever its extension. Owner and
  `ADMINISTRATOR` hold it, `manage` reads it, a Friend Shelf never carries it (no share column), so friends cannot reach
  `emu/`. Ceilings only shrink: it is a normal grant, no new bit, no overwrite.
- Migration `1800_emu_games_grant.sql`: `guild_storage_grants` and `storage_objects.kind` accept `view_games` / `games`.
- `crates/sr-storage/src/index/rom.rs`: platform and core by extension (`by_extension`) and by header (`detect`), the
  `RomRow`, `reachable_roms`. The indexer files `emu/roms/*` as class `rom`, kind `games`, `parsed = {title, platform,
  core}`; saves and BIOS files under `emu/` are never cataloged.
- `GET /users/@me/games/roms` (`routes/storage/games.rs`): the ROMs of the caller's own mounts and of guild shelves where
  a role holds `view_games`. Keys only; the browser presigns and reads from the bucket. Bots denied (`auth/policy.rs`).
- `POST /storage/mounts/{id}/presign` now also signs `PUT`, only for `emu/saves/<core>/<hash>/<file>` (exactly three
  segments) on the caller's OWN mount (not a shelf they reach, not staff).
- Role editor: the sixth toggle ("View shelf games"), `STORAGE_GRANTS` in `grantsApi.ts`.

**Web (`apps/web/src/emu/*`, route `/games`, lazy chunk, after sign-in only)**
- `loader.ts`: fetches `/emu/cores/<core>/<core>[-legacy]-wasm.data` (EmulatorJS 4.2.3 format found by emu-ops: a 7z
  archive), unpacks it with the `extract7z.js` worker, runs the glue as a blob script, builds the module. Refuses a
  response that is not 7z (so a dev server's index page is "core missing"). WebGL 2 check for the cores that need it.
- `session.ts`: drives the RetroArch exports (`simulate_input`, `save_state_info`, `load_state`, `cmd_savefiles`,
  `save_file_path`, ...). The core's own Web Audio driver plays the sound.
- `gamepad.ts` + `input.ts`: RetroPad mapping, Gamepad API polling, keyboard fallback, up to four players, the mapping
  screen's capture state machine, saved mapping (localStorage, tolerant of garbage and blocked storage).
  `MappingDialog.tsx` is the screen (press-a-key / press-a-button, skip, reset, table).
- `saves.ts`: `emu/saves/<core>/<rom-sha1>/sram.srm` and `state1..4.state`, written by presigned PUT to the member's own
  working mount (`pickSaveMount`), read back by presigned GET. SRAM is flushed every 30 s, on hide and on close.
- `bios.ts`: the PS1 BIOS rule (below). `BiosPanel.tsx` says why in the UI.
- `detect.ts`: core by extension and header (mirror of `rom.rs`). `play.ts`: prepare (read, detect, hash, cue tracks, SRAM).
- `EmuPage.tsx`: list (own bucket plus server shelves, own-bucket ROMs not yet indexed are added), player, save slots.
- Co-op seam (emu-coop's message): `coopOffer.ts` calls `offerCoopGame({title, setInput(port, button, value)})` and
  `withdrawCoopGame(game)` from `coop/source.ts` through `import.meta.glob`, so this branch builds without the co-op
  branch and starts offering the moment it is merged. Port is 0-based, button is the RetroPad index 0..15.
  The canvas is `canvas[data-testid="emu-canvas"]`.
- Test ROM: `scripts/make-test-rom.mjs` assembles `e2e/fixtures/emu/sideroom-test.nes` (24,592 bytes, committed). Screen
  red idle, blue while controller 1 holds A, green while controller 2 holds A (`$4017`), as the commander asked.
  `node scripts/make-test-rom.mjs --check` and `testRom.test.ts` keep file and script equal; the test also runs the ROM's
  code on a small 6502 model for the four pad combinations.
- D-385: `scripts/entrance-denylist.json` gained `/games`, `/emu`, `emulator`, `libretro`, `EmuPage`, `retroarch`,
  `view_games`. `node scripts/build-web.mjs` then `node scripts/check-entrance-isolation.mjs`: all ok; bundle budget ok.

**The PS1 BIOS rule**: only from `emu/bios/` of the member's own mount (`biosSourceAllowed`: user mount, same user, that
folder); read with `cache: 'no-store'`, `credentials: 'omit'`; exactly 512 KiB; digest on the known list (SHA-1 or MD5,
see open items); kept in memory for the visit, written only into the core's in-memory file system, never to
localStorage / IndexedDB / the Cache API / the server; PS1 will not start without it; the panel says why (Sony's
property, never shipped, stored or cached). A refused file shows its SHA-1 so the list can be extended deliberately.

## Verified in a real browser (not Playwright)
With the real FCEUmm core (digest equals the pinned `deploy/emu-cores.lock` row) and the self-written ROM, in the Browser
pane on a throwaway vite page on 5178 (since deleted, no port 8080/5173): the 7z unpack, the glue, `callMain`, frames
advancing, a red frame, player 1 A -> blue, player 2 A (port 1) -> green, `save_state_info` returned a 13,752-byte state,
`supports_states` true. This found three loader bugs (glue global cannot be deleted; `parent`/`callbacks` options;
core-options directory) fixed in commit 2892f138. The display lags by several hundred ms in a background pane, so the
spec polls.

## Tests run (all green)
- `cargo test -p sr-storage` (166 + doc), including grants/kinds/rom unit tests.
- Postgres (scratch DB `sr_p37_emushell`, `--test-threads=1`): `sr-db --test scopes_repo --test index_repo` (the sixth
  grant round trip, ROM rows filed as `games`/`rom`), `sr-server --test authz_matrix` (7: view_games listing/presign,
  save PUT own mount only, friend refused, unknown grant 400) and `--test immutable_rules` (13).
- `cargo test -p sr-api --lib` (778), `--test route_matrix` (6; the new route's row is `storage_scope`, bot deny),
  `--test openapi_names`; `cargo fmt --check` and `clippy -D warnings` on sr-storage, sr-db, sr-api, sr-server.
- OpenAPI, api-client types and MSW handlers regenerated (`mocks:check` ok).
- Web: `tsc --noEmit`, eslint, prettier, vitest `src/emu src/settings src/i18n src/storage src/router.test.tsx`
  (Vitest for the mapping, the save path and the BIOS rule: `gamepad.test.ts`, `saves.test.ts`, `bios.test.ts`, plus
  `detect`, `session`, `testRom`, `emuPage`). `e2e` `tsc --noEmit` ok.
- **Not run: Playwright.** `e2e/tests/emulator.spec.ts` is written and typechecked; it boots the NES core with the test
  ROM, samples the canvas (red, blue on A, red again), saves a state to a mocked bucket and opens the mapping screen. It
  skips itself when the core is not installed (`deploy/scripts/fetch-emu-cores.sh`). Run it when 8080/5173 are free.

## Still open, with reasons
- **No SNES and no Genesis** until a source for Beetle Supafaust and BlastEm exists (emu-ops 1.1: not in EmulatorJS
  4.2.3, and Snes9x / Genesis Plus GX / PicoDrive are rejected). The shell has both in its tables and shows "core not
  installed" for them. Owner decision.
- **BIOS list has MD5 values only.** The lists the agents could read (libretro docs, DuckStation) publish MD5; no SHA-1
  was verifiable offline, so none was invented. `checkBios` accepts a file whose SHA-1 OR MD5 is listed (two sources agree
  on each MD5) and `BiosEntry.sha1` exists for values derived from the owner's own dumps. If the owner wants SHA-1-only,
  they derive them from a dump they own (the panel shows the SHA-1 of any refused file) and fill the list.
- PCSX-ReARMed has a built-in HLE BIOS; the shell still demands a real one (conservative, as the roadmap says). Owner may
  relax this.
- The indexer's catalog lags until the next `storage.index` run; the page adds own-bucket ROMs by listing `emu/roms/`.
  A guild shelf's new ROMs appear after the index run.
- Friend shelves do not carry games (no `games` column on `storage_mount_shares`). Add one if friends should share ROMs.
- Save states are slot 1..4 only; no autosave state, no rewind, no fast-forward, no netplay.
- A production CSP for the app shell (if one is added) needs `script-src 'self' blob: 'wasm-unsafe-eval'`, `worker-src
  'self' blob:`, and the bucket in `connect-src`; today the app shell has none, only the public shells do.
- `THIRD_PARTY_NOTICES.md` and `check-licenses.mjs` are already done for the three available cores by emu-ops
  (`feat/p37-emu-ops`), so this branch does not touch them (avoids a merge conflict).

## Decisions
- ROM list through a dedicated route rather than a new shelf `Section` (sections are special-cased in every shelf query).
- Core ids are the directory names under `/emu/cores/`: `fceumm`, `supafaust`, `mgba`, `blastem`, `pcsx_rearmed`.
- Saves go only to an own, active, CORS-checked mount; with none, saving is off and the UI says nothing is stored on the
  instance.

## Candidate homebrew ROMs for the owner (nothing downloaded; from emu-ops' verified research, 2026-10-31)
| Name | Platform | Author | Licence | Official URL | Size |
|---|---|---|---|---|---|
| Nova the Squirrel | NES | NovaSquirrel | GPL-3.0 (repo LICENSE) | https://github.com/NovaSquirrel/NovaTheSquirrel (release `v1.0.6a`, `nova.nes`) | 262,160 B |
| Solar Guard (GBA Jam 2021) | GBA | Deft-Spade | GPL-3.0 (GitHub) | https://github.com/Deft-Spade/Solar-Guard (release `GBA-JAM-2021`) | 4,272,776 B |
| Super Tilt Bro | NES | sgadrat | WTFPL repo; game assets unchecked; built from source | https://github.com/sgadrat/super-tilt-bro | unknown |
| 2048 (NES) | NES | tsone | MIT per the roadmap; repo not verified | owner to locate | about 40 KB |
| Anguna | GBA | unverified | GPL-2.0 code per the roadmap, assets open | owner to locate | about 4 MB |
The owner confirms the licence of art, music and bundled assets before any ship.

## Owner questions
1. SNES and Genesis: accept "not installed", name another source for Supafaust and BlastEm, or relax the rejected list?
2. BIOS: keep "real BIOS required for PS1", or allow PCSX-ReARMed's HLE BIOS? Fill SHA-1 values from your own dump?
3. Friends: should a Friend Shelf be able to share games?
4. Which homebrew ROMs (above), if any, ship in the repo?

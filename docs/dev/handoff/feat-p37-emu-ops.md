# Handoff, wave 37 emu-ops (ACT-EMU-003): cores by digest at install time

Branch `feat/p37-emu-ops`. Not pushed. This file starts with the research findings emu-shell needs; the work log
(done, open, tests, owner questions) is at the end.

## 1. Format findings (read this first, emu-shell)

Source: EmulatorJS **4.2.3**, `https://cdn.emulatorjs.org/4.2.3/data/`. The downloaded files live OUTSIDE the repo in
`C:\Users\braxt\Desktop\SIDEROOM_wt\p37-cores-download` (never run, never imported, never committed). The exact URLs,
sizes and SHA-256 digests are the table in [deploy/EMULATOR.md](../../../deploy/EMULATOR.md).

### 1.1 Only three of the five allowed cores exist in this build set (owner decision needed)

`data/cores/cores.json` of 4.2.3 lists 40 cores. Of the five allowed ones it contains:

| Allowed core | In EmulatorJS 4.2.3 | Pinned |
|---|---|---|
| FCEUmm (NES, GPL-2.0) | yes, `fceumm` | yes |
| mGBA (GBA, GB, GBC, MPL-2.0) | yes, `mgba` | yes |
| PCSX-ReARMed (PS1, GPL-2.0) | yes, `pcsx_rearmed` | yes |
| Beetle Supafaust (SNES, GPL-2.0) | **no**: the `supafaust`, `mednafen_supafaust`, `beetle_supafaust` and `mednafen_snes` URLs are all 404; the build set has `snes9x` only | no |
| BlastEm (Genesis, GPL-3.0) | **no**: `blastem-wasm.data` is a 404; the build set has `genesis_plus_gx` and `picodrive` only | no |

`snes9x`, `genesis_plus_gx` and `picodrive` are on the rejected list (non-commercial), so they were NOT downloaded.
Consequence: with this source the instance has NES, GB, GBC, GBA and PS1, and **no SNES and no Genesis**. Getting
Supafaust and BlastEm needs a different source (an Emscripten build of those two libretro cores, which the plan did
not schedule) or the owner relaxing the list. The fetch script and the table are built so a row can be added later.

### 1.2 On-disk format: a 7z archive per core, not a .js/.wasm pair

Each core is ONE file `<core>-wasm.data` (plus a `<core>-legacy-wasm.data` twin), about 1 MB, which is a **7z archive**
(magic `37 7A BC AF 27 1C`). For fceumm it holds `fceumm_libretro.js` (277 KB), `fceumm_libretro.wasm` (4.5 MB),
`build.json` (`{"minimumEJSVersion":"4.2.2","version":"2.0.2"}`), `core.json` (name, extensions, `save` = `srm`,
`options`, repo) and `license.txt` (the licence text); mgba and pcsx_rearmed hold the same five members. So the browser
must unpack before it can run anything. EmulatorJS does it like this (`data/src/compression.js` and
`downloadGameCore` in `data/src/emulator.js`):

1. Fetch the `.data` as an ArrayBuffer. Sniff the magic (`7z`, `zip` = `PK`, `rar`).
2. For `7z`: load the helper `compression/extract7z.js` (280 KB, asm.js, **self-contained, no separate .wasm**) and
   start it as a **Worker** (EmulatorJS wraps the text in a Blob), then `worker.postMessage(uint8array)`. It answers
   with `{t:4,current,total}` (progress), `{t:2,file,size,data}` (one per member, `data` a Uint8Array) and `{t:1}` (done).
3. Pick the members by suffix: `.wasm` -> wasm bytes, `.js` (not `.worker.js`) -> glue script, plus `build.json`,
   `core.json`, `license.txt`.
4. Run the glue: Blob of the js -> `<script src=blob:...>`; the script defines a global **`window.EJS_Runtime`** (an
   Emscripten module factory). Then
   `EJS_Runtime({ noInitialRun:true, canvas, locateFile(f){ for .wasm return URL.createObjectURL(new Blob([wasm],{type:'application/wasm'})) }, getSavExt:()=>'.srm', print, printErr })`
   resolves to the module. The ROM bytes are written with `Module.FS.writeFile('/'+name, bytes)`, a `retroarch.cfg` is
   written to `/home/web_user/.config/retroarch/retroarch.cfg`, then `Module.callMain(['/'+name])` and
   `Module.resumeMainLoop()` start it.
5. These are **RetroArch Emscripten builds patched by EmulatorJS**, not bare libretro cores. The glue exports C
   functions reached with `Module.cwrap` (see `data/src/GameManager.js`, 464 lines): `simulate_input(player,
   buttonIndex, value)`, `save_state_info`, `load_state(path,size)`, `supports_states`, `cmd_savefiles`,
   `refresh_save_files`, `save_file_path`, `system_restart`, `toggleMainLoop(0|1)`, `get_core_options`,
   `ejs_set_variable(key,value)`, `toggle_fastforward`, `toggle_rewind`, `get_video_dimensions`, `set_vsync`,
   `ejs_set_keyboard_enabled`, `get_current_frame_count`, `get_disk_count`. A save state travels through the file
   `/game.state` (write it, then `load_state`). Audio is RetroArch's own Web Audio driver inside the glue. So emu-shell
   does NOT implement libretro callbacks; it reproduces the thin EmulatorJS glue above (about 300 lines). It must NOT
   import EmulatorJS's `loader.js`/`emulator.js` (that brings its whole UI and a CDN fallback).
6. **Which variant**: EmulatorJS fetches `<core>-legacy-wasm.data` when the core's report
   (`cores/reports/<core>.json`) has no `options.defaultWebGL2`, else `<core>-wasm.data`. That means `fceumm` and
   `mgba` -> **legacy**, `pcsx_rearmed` (`defaultWebGL2:true`) -> **non-legacy** (needs WebGL2). Both variants of the
   three cores are pinned, so the shell can follow the same rule.

### 1.3 What the loader must do (and the platform consequences)

- Layout on the served side (what `fetch-emu-cores.sh` writes, under `apps/web/public/emu/cores` in dev):
  `<core>/<core>-wasm.data`, `<core>/<core>-legacy-wasm.data` and `extract7z.js` at the top. The 7z unpack happens in
  the browser, so the installer needs no 7z tool. Start the worker with `new Worker('/emu/cores/extract7z.js')`: a
  same-origin URL works, no Blob needed.
- Content-Security-Policy: the glue script and the wasm come from **blob:** URLs and WebAssembly is compiled, so the
  emulator page needs `script-src 'self' blob:` and `'wasm-unsafe-eval'` (plus `worker-src 'self' blob:` if the Blob
  worker is kept). Apply that only on the route group that hosts the emulator, never on the entrance (D-385: nothing
  emulator-related before sign-in; the lazy emulator chunk fetches the cores only after sign-in).
- Load order on "Play": fetch `.data` -> sniff magic -> extract7z worker -> pick members -> script tag ->
  `EJS_Runtime(...)` -> write ROM -> `callMain` -> `resumeMainLoop`. Cache the unpacked members in IndexedDB keyed by
  the pinned digest if a warm start is wanted.
- Never use the EmulatorJS CDN fallback; cores come only from the instance's own `/emu/cores/`.
- `license.txt` inside every archive is the core's licence; `core.json.repo` is the corresponding-source link
  (`EmulatorJS/libretro-fceumm`, `EmulatorJS/mgba`, `EmulatorJS/pcsx_rearmed`), the Q23 offer.
- Extensions per core (`core.json`): fceumm `fds nes unif unf`; mgba `gb gbc gba`; pcsx_rearmed
  `bin cue img mdf pbp toc cbn m3u ccd` (PS1 needs a BIOS written into the module FS before `callMain`).
- Saves: SRAM is a plain `.srm` file, read with `Module.FS.readFile(save_file_path())` and written back before
  `refresh_save_files`. EmulatorJS also mounts IDBFS at `/data/saves`; the shell should not use it (saves go to the
  member's mount by presigned PUT).
- Dev serving: the cores are untracked (`apps/web/public/emu/cores/` is in `.gitignore`). A production build copies
  `public/` into `dist/`, so for prod fetch into the served directory AFTER the build (`--dest`), not before it.

## 2. Candidate homebrew ROMs for the owner (nothing was downloaded)

Pick one or two; the owner confirms each licence before anything ships. "Verified" = read from the project's GitHub
metadata on 2026-10-31; the licence of art, music and bundled assets must still be confirmed by the owner.

| # | Name | Platform | Author | Licence | Official URL | Size |
|---|---|---|---|---|---|---|
| 1 | Nova the Squirrel | NES | NovaSquirrel | GPL-3.0 (repo `LICENSE.txt`, GitHub spdx GPL-3.0; verified) | https://github.com/NovaSquirrel/NovaTheSquirrel (ROM: https://github.com/NovaSquirrel/NovaTheSquirrel/releases/download/v1.0.6a/nova.nes) | 262,160 bytes (verified, release asset) |
| 2 | Solar Guard (GBA Jam 2021) | GBA | Deft-Spade | GPL-3.0 (GitHub spdx; verified) | https://github.com/Deft-Spade/Solar-Guard (ROM: https://github.com/Deft-Spade/Solar-Guard/releases/download/GBA-JAM-2021/Solar_Guard_GBA_JAM_2021.gba) | 4,272,776 bytes (verified, release asset) |
| 3 | 2048 (NES) | NES | tsone | MIT per roadmap section 19; the repo was not found by the GitHub search, NOT verified | to be located by the owner (roadmap candidate) | about 40 KB per roadmap |
| 4 | Anguna | GBA | not verified | GPL-2.0 code per roadmap, assets licence open (the owner's own condition) | to be located by the owner (roadmap candidate) | about 4 MB per roadmap |
| 5 | Super Tilt Bro | NES | sgadrat | WTFPL for the repo (GitHub spdx; verified), game assets not checked; no prebuilt ROM release found, it is built from source (`build.sh`) | https://github.com/sgadrat/super-tilt-bro | not known |

Recommendation: 1 (NES) and 2 (GBA) are the only two with a verified licence and a direct release asset. Both are
GPL-3.0, consistent with D-309 next to a GPL web bundle, but the owner should accept that. For the co-op spec (a ROM
that reacts to a button) any of them works. The tests do NOT use these: they use a ROM the agent generates itself.

## 3. Work log

(filled in below as the item progresses)

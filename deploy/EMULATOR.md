# Emulator cores: install, pins and licences (ACT-EMU-003)

The in-browser emulator (wave 37) runs prebuilt WebAssembly cores. **The binaries are never committed to this
repository.** An installer fetches them from a pinned URL and refuses any file whose SHA-256 differs from the table
below. Nothing emulator-related loads before sign-in (D-385); the cores are fetched by the browser only after sign-in.

| What | Where |
|---|---|
| Fetch script (POSIX sh, needs `curl` and `sha256sum`/`shasum`) | `deploy/scripts/fetch-emu-cores.sh` |
| Same thing from the operator CLI (needs `curl`; no config, no database) | `sr-cli emu fetch-cores` |
| The pin list both read (the table below is generated from it and checked against it) | `deploy/emu-cores.lock` |
| Optional installer step | `SR_INSTALL_EMU_CORES=yes deploy/install.sh` (or answer the prompt) |
| Dev directory (untracked) | `apps/web/public/emu/cores/` |
| Production directory | `$SR_WEB_DIR/emu/cores/` (the installer's web directory) |

## Fetching

```sh
# development checkout
deploy/scripts/fetch-emu-cores.sh                       # into apps/web/public/emu/cores
sr-cli emu fetch-cores --dest apps/web/public/emu/cores # same result

# production: after copying the built web app into the web directory
deploy/scripts/fetch-emu-cores.sh --dest /opt/sideroom/web/emu/cores
deploy/scripts/fetch-emu-cores.sh --dest /opt/sideroom/web/emu/cores --verify-only   # exit 1 when a file differs
```

- Safe to repeat: a file that already matches its digest is skipped; `--force` downloads again.
- Each file is downloaded to `<name>.part`, hashed, and moved into place only when digest and size match. A wrong
  file is deleted and the command exits 1, so an unverified file is never served.
- https only, with https-only redirects and a size cap. `--base-url` points at a mirror (https), or at
  `http://127.0.0.1:<port>` for tests.
- Re-deploying the web app must keep `emu/cores/` (copy the build over it without deleting the folder) or run the
  script again. A production build copies `apps/web/public/` into `dist/`; fetch into the served directory AFTER the
  build, never before it, so the cores do not end up in the artifact.
- Dev and prod must serve the files **as they are**; the browser unpacks them (see "Format").

## Pinned files (EmulatorJS 4.2.3, `https://cdn.emulatorjs.org/4.2.3/data/`)

Verified on 2026-10-31 by downloading each file once (size and SHA-256 below).

| File (under the cores directory) | URL | Bytes | SHA-256 | Licence |
|---|---|---|---|---|
| `extract7z.js` | `https://cdn.emulatorjs.org/4.2.3/data/compression/extract7z.js` | 280155 | `4ac9933b995a516cb6b3ca4027db860278372a20c21c405d93ceb1998498853a` | GPL-3.0 (shipped in EmulatorJS 4.2.3; 7-Zip port, upstream licence unconfirmed) |
| `fceumm/fceumm-wasm.data` | `https://cdn.emulatorjs.org/4.2.3/data/cores/fceumm-wasm.data` | 1054015 | `8c449fd5c36646fb0769423ed6ffa9efbdfc21fbfdc9bac7952b559d34d5b493` | GPL-2.0 |
| `fceumm/fceumm-legacy-wasm.data` | `https://cdn.emulatorjs.org/4.2.3/data/cores/fceumm-legacy-wasm.data` | 1053006 | `f1054b094e7149fd6278485bc1b2e51ff75c5259048ddb1134171e53d651f239` | GPL-2.0 |
| `mgba/mgba-wasm.data` | `https://cdn.emulatorjs.org/4.2.3/data/cores/mgba-wasm.data` | 1055616 | `01fcaf6d4296ef1db6676e0c69400c4474e24572d0b2b99cc097e4ae885e02d7` | MPL-2.0 |
| `mgba/mgba-legacy-wasm.data` | `https://cdn.emulatorjs.org/4.2.3/data/cores/mgba-legacy-wasm.data` | 1054993 | `e300d2f368c82bf61d77c972c36e367cf16408ec3d50beede080337519a3d696` | MPL-2.0 |
| `pcsx_rearmed/pcsx_rearmed-wasm.data` | `https://cdn.emulatorjs.org/4.2.3/data/cores/pcsx_rearmed-wasm.data` | 1039627 | `fe5515f6c29f093f0e8c01824b213804f1f76eb9cb4c97c72fe2cc17606bfbc2` | GPL-2.0 |
| `pcsx_rearmed/pcsx_rearmed-legacy-wasm.data` | `https://cdn.emulatorjs.org/4.2.3/data/cores/pcsx_rearmed-legacy-wasm.data` | 1038636 | `88faee73d06c597a68a8b78e5621d97feefca134c64b623e65f0a900037456da` | GPL-2.0 |

About 7.6 MB in all. To change a pin: edit `deploy/emu-cores.lock` and this table together; `node
scripts/check-ops-files.mjs` fails when they disagree.

## Which cores, and which are missing

| Core | System | Licence | In the table |
|---|---|---|---|
| FCEUmm | NES, Famicom Disk System | GPL-2.0 | yes |
| mGBA | Game Boy, Game Boy Color, Game Boy Advance | MPL-2.0 | yes |
| PCSX-ReARMed | PlayStation (BIOS supplied by the member, never stored) | GPL-2.0 | yes |
| Beetle Supafaust | SNES | GPL-2.0 | **no**: not in the EmulatorJS 4.2.3 build set (the CDN has `snes9x` only) |
| BlastEm | Genesis / Mega Drive | GPL-3.0 | **no**: not in the EmulatorJS 4.2.3 build set (the CDN has `genesis_plus_gx` and `picodrive` only) |

Rejected and deliberately absent: Snes9x, Genesis Plus GX, PicoDrive, FBNeo, every arcade core and every other
non-commercial core (docs/dev/roadmap-p27.md sections 19 and 20). Until a legal source of Supafaust and BlastEm builds is
chosen, an instance has no SNES and no Genesis. The RetroArch frontend compiled into every core is GPL-3.0.

## Format

Each `.data` file is a **7z archive** (about 1 MB) of five members: `<core>_libretro.js` (the Emscripten glue, which
defines `EJS_Runtime`), `<core>_libretro.wasm` (about 4 MB), `build.json`, `core.json` (name, file extensions, repository
link) and `license.txt` (the licence text). The browser unpacks it with `extract7z.js` in a Worker; the server and
the installer never unpack anything. `<core>-legacy-wasm.data` is the same core built without the WebGL2 requirement:
fceumm and mgba use the legacy file, pcsx_rearmed the other one, as EmulatorJS does. These are RetroArch builds patched
by EmulatorJS; the loader contract is in `docs/dev/handoff/feat-p37-emu-ops.md`.

## Licences and the corresponding source

The cores are GPL-2.0, GPL-3.0 and MPL-2.0 software served to members as separate files (as JASSUB and ffmpeg.wasm
are). The licence text of each core is inside its archive and in `THIRD_PARTY_NOTICES.md`; `scripts/check-licenses.mjs`
allows each by name. The corresponding source is the project each archive names in `core.json`:

| Core | Source |
|---|---|
| FCEUmm | https://github.com/EmulatorJS/libretro-fceumm |
| mGBA | https://github.com/EmulatorJS/mgba |
| PCSX-ReARMed | https://github.com/EmulatorJS/pcsx_rearmed |
| Build set and frontend | https://github.com/EmulatorJS/EmulatorJS (tag v4.2.3) |

## PlayStation BIOS

The PlayStation core needs Sony's BIOS, which is Sony's copyright. The instance never ships, stores or caches it: a
member supplies it from their own storage mount (`emu/bios/`), the shell checks its hash against a known list and
holds it in memory only. Only ROMs a member owns, and homebrew, are legal to play (docs/dev/roadmap-p27.md Q23).

## ROMs

No ROM of any kind is downloaded or committed by this item. Test ROMs are generated by the test code itself.

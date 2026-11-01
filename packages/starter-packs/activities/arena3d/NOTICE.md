# Arena 3D: provenance and notice

Arena 3D (`ACT-A3D-001`) is SIDE ROOM's own 3D starter kit, written for SIDE ROOM in wave 46.

## Code

- Everything under `sim/`, `net/`, `render/`, `assets/`, `client/`, `platform/`, `replay/`, `tools/` and `test/`
  was written for this project. The platform, channel and bundle patterns follow Land.io
  (`../landio/`, the owner's own game), and `tools/bundle.mjs` is adapted from `landio/tools/bundle.mjs`.
- The math helpers (`render/math.js`, `sim/dmath.js`) are written here; no third-party math library is used.

## Third-party code in the bundle

| What | Version | Licence | Where in the bundle |
|---|---|---|---|
| Jolt Physics, WebAssembly port (`jolt-physics` on npm, by Jorrit Rouwe) | 1.1.1, `wasm-compat` build | MIT | `vendor/jolt-physics.wasm-compat.js`, licence text in `vendor/jolt-physics.LICENSE.txt` |
| SIDE ROOM Embedded App SDK (`packages/embedded-app-sdk`, this repository) | workspace | this repository's licence | `sdk/*.js` |

`tools/bundle.mjs` rewrites two strings in the vendored Jolt file so that it passes the bundle rules. Neither
changes behaviour in a browser; see the comment on `JOLT_REWRITES` in `tools/bundle.mjs`:

1. The source URL in the file's header comment loses its scheme (the rules refuse any absolute URL).
2. The Node-only `await import("node:module")`, which runs only when `process.versions.node` is set, is
   replaced by an object whose `createRequire` throws (the rules refuse bare import specifiers).

## Assets

No third-party models, textures, fonts or sounds are used. Every asset was made here:

- **Meshes:** boxes, capsules and the arena are generated in code (`render/meshes.js`), and the sample model
  `assets/models/beacon.json` is a glTF 2.0 file written by `assets/make-beacon.mjs` in this folder
  (procedural geometry, base64 buffer).
- **Textures:** the floor grid is drawn on a canvas at run time (`render/textures.js`).
- **Sounds:** footsteps, jumps and bumps are synthesised with Web Audio (`client/sfx.js`).
- **Font:** the system UI font stack. No font files are bundled.
- **Names:** the bot names `Lumen bot` and `Tofu bot` come from the P28 roadmap.

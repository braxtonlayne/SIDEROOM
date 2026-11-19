# Handoff: fix/p115-assets (wave 115, D-1049, VOX-CLI-002)

## Clock times

Started 06:09 on 2026-11-19, commander approval of the 16 packages mid-task, ready at about 06:22.

## Done (commits)

- `D-1049 ... vendored` commit: the model, its licence, pins, the runtime dependency, `assets.ts`, the asset check and its test, the notices.
- A second commit tightens the CDN rule of the check (see "check (d)") and holds this handoff.

## DOWNLOADS

1. Model: `silero_vad.onnx`, https://raw.githubusercontent.com/snakers4/silero-vad/v5.1.2/src/silero_vad/data/silero_vad.onnx, tag `v5.1.2` (newest v5 release tag; v6.0 to v6.2.3 exist and were not used, on purpose: the task is written against v5), 2,327,524 bytes, SHA-256 `2623a2953f6ff3d2c1e61740c6cdb7168133479b267dfef114a4a3cc5bdd788f`. A real ONNX file, not an LFS pointer.
   Its `LICENSE` at the same tag: MIT (confirmed from the text, "Silero Team"), 1,075 bytes, SHA-256 `2e63e9a38b6e8fc0c7bc37ce174caca1862870856c6daf5697cfb785e925520b`. No other file of that repository.
2. Runtime: npm `onnxruntime-web` 1.31.1 (npm `latest`, stable, MIT), added with `--save-exact` through `npx pnpm@10.12.1` (the tree's own way; plain `pnpm` is not on PATH). The tarballs are from the registry; their integrity hashes are in `pnpm-lock.yaml`.

**No install or lifecycle script ran** for any of the 16 packages: pnpm 10 skips dependency scripts that are not listed, `onlyBuiltDependencies` in `pnpm-workspace.yaml` is untouched (esbuild only), and protobufjs's `postinstall` therefore did not run.

## The lock's diff (105 lines added, none removed; no existing package moved)

onnxruntime-web 1.31.1 (MIT), onnxruntime-common 1.31.1 (MIT), protobufjs 7.6.6 (BSD-3-Clause), @protobufjs/aspromise 1.1.2, base64 1.1.2, codegen 2.0.5, eventemitter 1.1.1, fetch 1.1.1, float 1.0.2, path 1.1.2, pool 1.1.0, utf8 1.1.2 (all BSD-3-Clause), flatbuffers 25.9.23 (Apache-2.0), long 5.3.2 (Apache-2.0), guid-typescript 1.0.9 (ISC), platform 1.3.6 (MIT). That is 16; the task's limit was 10, allowed by the commander (Fable).

## Not done / left

- TYPECHECK: left to CI (memory: about 1.1 GB free against the 3.3 GB a web tsc needs). Vitest, eslint: left to the commander. The build (esbuild) compiled `assets.ts` and the `?url` imports; types are unchecked.
- `web.yml` and `release.yml` are not mine; the two lines for the commander are below.
- No `vite.config.ts` or `build-web.mjs` change was needed. No `bundle-budget.json` entry was needed (the runtime chunk is 25.8 KB gzip).

## Design choices

- Runtime import for vad: `import('onnxruntime-web/wasm')`, the package's own bundle build (`ort.wasm.bundle.min.mjs`). It holds the runtime's glue inside the chunk, so there is no separate glue `.mjs` file in dist, and only the plain one-thread CPU wasm exists. The extern-wasm build (`ort.wasm.min.mjs`) would need a Vite `resolve.conditions` change and a second `?url`; not worth it.
- `assets.ts` has two `?url` imports (model, wasm) and one exported object. Nothing in the tree imports it yet (vad will). Vite emits the two files only once something imports `assets.ts`.

## Proof of point 2 (lazy, nowhere early)

Proven with a temporary probe (not committed): a `probe.ts` that imports `assets.ts` and `import('onnxruntime-web/wasm')`, reached by one dynamic import in `app/main.tsx`, built as job `0356-assets-build`; both edits were then reverted. Built files:

```
app/ort-wasm-simd-threaded-qh98TCA_.wasm   14,848,852 B   (identical to node_modules dist/ort-wasm-simd-threaded.wasm)
app/silero_vad-Dt9-bTDn.onnx                2,327,524 B   (identical to the vendored model)
app/ort.wasm.bundle.min-Ba0FTpUY.js            77,326 B   (the runtime's own chunk)
```

`grep -rlE "ort-wasm|silero_vad|onnxruntime"` over every `.js .mjs .html .json .webmanifest` in dist gives only `app/ort.wasm.bundle.min-*.js` and `app/probe-*.js`. `sw.js`: 0 hits. No jsep, jspi or asyncify file exists in dist. First paint: `app/app-BuMRy1zN.js` 218,413 B gzip against the base build's 218,457 B (the base dist `p115-base-dist`, measured the same way; the base figure of 218,467 in the task is another build). The difference is the commit id and chunk hashes inside the entry chunk, not a membership change: the only change to an existing file in the probe build was one dynamic import, and the three checks below pass.

## Sizes for the plan (point 7)

| File | raw | gzip -9 |
| --- | --- | --- |
| model (`silero_vad.onnx`) | 2,327,524 | 1,943,500 |
| wasm (`ort-wasm-simd-threaded.wasm`) | 14,848,852 | 3,799,594 |
| glue | inside the runtime chunk (about 24 KB raw of its 77 KB) | n/a |
| runtime chunk (`ort.wasm.bundle.min.js`) | 77,326 | 25,801 |

A first Silero call therefore fetches about 17.3 MB raw, about 5.8 MB gzip (the server's compression decides which). The wasm is the cost to weigh.

## FOR vad

- Import: `import { sileroAssets } from './assets';` giving `{ modelUrl, wasmUrl }` (both root-absolute paths such as `/app/silero_vad-<hash>.onnx`).
- Runtime: `const ort = await import('onnxruntime-web/wasm');` version 1.31.1. Before creating a session: `ort.env.wasm.numThreads = 1; ort.env.wasm.wasmPaths = { wasm: sileroAssets.wasmUrl };` (set `wasmPaths` before anything else; do not set `.mjs`: the glue is in the chunk). `ort.env.wasm.proxy` stays false. Inside the worker, `wasmUrl` is a path on our origin; resolve it with `new URL(wasmUrl, self.location.origin)` if the runtime wants an absolute URL. Not yet run in a browser: vad's first job must show the session creates under the CSP (`wasm-unsafe-eval`).
- Wasm file name: `ort-wasm-simd-threaded-<hash>.wasm`; the model: `silero_vad-<hash>.onnx`.
- Model I/O (read with onnxruntime-web 1.31.1 in Node, see `apps/web/vendor/silero-vad/SOURCE.md`): inputs `input` float32 `[batch, samples]`, `state` float32 `[2, batch, 128]` (zeros to start), `sr` int64 scalar (16000, a `BigInt64Array` of one, dims `[]`); outputs `output` float32 `[batch, 1]` (probability) and `stateN` float32 `[2, batch, 128]` (feed back as `state`). Run once in Node: `input [1,512]` gave `output [1,1]` and `stateN [2,1,128]`. Whether the 64-sample context (576 input) is needed is not verified here: SOURCE.md says so.
- Dev server: `optimizeDeps.exclude` was not touched; if the dev pre-bundler breaks `onnxruntime-web/wasm`, add it to the exclude list in `vite.config.ts` (not mine now).

## CI lines for the commander (web.yml and release.yml are not mine)

After the step that builds the web client and runs `node scripts/check-bundle-budget.mjs`, in both workflows:

```
      - name: VAD assets (pinned, self-hosted, lazy)
        run: node scripts/check-vad-assets.mjs
```

and add `node --test scripts/check-vad-assets.test.mjs` next to where the other `scripts/*.test.mjs` run. The check needs a build in which something imports `assets.ts` (vad's code): on this branch alone it fails with (b) "expected one built silero_vad*.onnx, found 0".

## Check (scripts/check-vad-assets.mjs)

(a) the vendored model against `SHA256SUMS`; (b) exactly one built `silero_vad*.onnx` equal to the vendored file, exactly one `ort-wasm-simd-threaded-<hash>.wasm`, and a failure on any other `ort-*` file (jsep, jspi, asyncify, webgpu, webgl, a second wasm or `.mjs`) or any other `.onnx`; (c) no marker (`ort-wasm`, `silero_vad`, `onnxruntime`, the two built file names) in any file at the top of dist, under `entrance/`, in the static closure of any root `.html` shell, or in `sw.js`; (d) no CDN URL for the runtime in any built JavaScript. Check (d) only flags jsdelivr or unpkg URLs that name onnxruntime, or `onnxruntime` followed by a host: the base build has an unrelated `https://unpkg.com/@ffmpeg/core...` default in ffmpeg's worker (overridden at run time), which a plain "unpkg.com" rule would wrongly flag.

## FINDERS (rule 13)

`git grep -n "silero\|onnxruntime"` in `apps/web/src packages scripts e2e`: no hit outside my own files. Tests that read `THIRD_PARTY_NOTICES.md` as text: `check-notices-copy.mjs` (passes, copy regenerated with `--write`), `split-notices.mjs` (section name `onnxruntime-web` is not in any public JS, so the section stays in the gated copy; `check-entrance-isolation` passes).

## JOBS

| id | class | waited | ran | rc |
| --- | --- | --- | --- | --- |
| 0356-assets-build | build (with the temporary probe, since reverted) | 4 s | 19 s | 0 |

## Checks (own tree)

- `node --test scripts/check-vad-assets.test.mjs`: 10 pass, 0 fail.
- `node scripts/check-vad-assets.mjs` on the probe build: ok after the (d) refinement. `check-bundle-budget.mjs`, `check-entrance-isolation.mjs`, `check-staff-delivery.mjs` on that dist: rc 0 each.
- `pnpm licenses list --json --prod | node scripts/check-licenses.mjs`: "Licence check ok", rc 0. `node scripts/check-notices-copy.mjs`: identical.
- `npx prettier --check` on my files: clean. Web tsc, vitest, eslint: left to the commander.

## Notices

One section appended to `THIRD_PARTY_NOTICES.md` (copied to `apps/web/public/THIRD_PARTY_NOTICES.md` with `check-notices-copy.mjs --write`): "onnxruntime-web 1.31.1 and Silero VAD", with the licence text of every package (the Silero MIT text from the vendored file; the MIT text of Microsoft for the two onnxruntime packages, which ship no LICENSE file, their `package.json` says MIT; platform; the ISC text for guid-typescript, which ships none either, `package.json` says ISC; the BSD-3-Clause text of protobufjs and one identical text for the nine `@protobufjs/*` helpers; the Apache-2.0 text for flatbuffers and long). The Microsoft and guid-typescript texts are the standard texts with the holder taken from the package metadata; flag them if the owner wants the upstream repositories' texts.

## Closing text for my rows

D-1049: the Silero VAD model (v5.1.2, MIT, pinned by SHA-256) and `onnxruntime-web` 1.31.1 are in the tree; the build emits the model and the one-thread CPU wasm as hashed static files of the app's own output; `scripts/check-vad-assets.mjs` keeps them pinned, self-hosted, single-variant and out of first paint.

Changelog sentence: Added the pinned Silero voice-activity model and the ONNX runtime as self-hosted, lazily loaded files, not yet used by any feature.

## Owner questions

None. (The 14.8 MB wasm is the main cost; a smaller runtime would mean a different engine and is Fable's call.)

## PROCESSES

`strays: none found`.

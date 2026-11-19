# Silero VAD model (D-1049)

- Project: https://github.com/snakers4/silero-vad (Silero Team, MIT; `LICENSE` beside this file is the repository's, unmodified).
- Tag: `v5.1.2` (the newest v5 release tag on 2026-11-19). v6.x tags (v6.0 to v6.2.3) existed that day; v5.1.2 was chosen on purpose, because the detector is written against the v5 interface below.
- Model: `src/silero_vad/data/silero_vad.onnx` at that tag, fetched once on 2026-11-19 from
  `https://raw.githubusercontent.com/snakers4/silero-vad/v5.1.2/src/silero_vad/data/silero_vad.onnx`.
  Licence: `LICENSE` at the same tag, fetched from `https://raw.githubusercontent.com/snakers4/silero-vad/v5.1.2/LICENSE`.
- No other file of that repository is in the tree. Nothing is fetched at build time or run time from there.

| File | Bytes | SHA-256 |
| --- | --- | --- |
| `silero_vad.onnx` | 2327524 | `2623a2953f6ff3d2c1e61740c6cdb7168133479b267dfef114a4a3cc5bdd788f` |
| `LICENSE` | 1075 | `2e63e9a38b6e8fc0c7bc37ce174caca1862870856c6daf5697cfb785e925520b` |

`SHA256SUMS` holds the same two sums (`sha256sum -c SHA256SUMS`); `scripts/check-vad-assets.mjs` checks the model against it
and against the built copy.

## Interface (read from the model's own metadata with onnxruntime-web 1.31.1 on 2026-11-19, then run once)

Inputs:

- `input`: float32, `[batch, samples]`
- `state`: float32, `[2, batch, 128]`
- `sr`: int64, scalar (`[]`), 16000

Outputs:

- `output`: float32, `[batch, 1]` (speech probability)
- `stateN`: float32, `[2, batch, 128]` (feed it back as the next `state`; start from zeros)

Verified by running the session: `input` `[1, 512]` with `state` `[2, 1, 128]` zeros and `sr` 16000 gives `output` `[1, 1]` and `stateN` `[2, 1, 128]`. The frame size is not read from the model (it accepts any length). From the project's v5 usage as recalled, not fetched or checked here: 512 samples at 16 kHz (32 ms), and the project's own wrapper prepends the last 64 samples of the previous frame as context (a 576-sample input). vad must check which of these gives sensible probabilities on speech.

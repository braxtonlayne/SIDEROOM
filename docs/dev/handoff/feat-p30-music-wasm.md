# feat/p30-music-wasm - MUS-WASM-001

## Done
- `crates/sr-music-wasm` (new, no new dependencies: wasm-bindgen, serde_json, thiserror already in Cargo.lock; Cargo.lock only gains the crate itself).
- Hand-written readers: ID3v2.2/2.3/2.4 (text frames, TRCK/TPOS, TDRC/TYER, TCON incl. "(17)", TLEN, APIC/PIC front-cover preference, whole-tag and per-frame unsynchronisation, extended header, data-length indicator, Latin-1/UTF-16 BOM/UTF-16BE/UTF-8), ID3v1/1.1 tail, FLAC (STREAMINFO duration, Vorbis comments, PICTURE), Ogg Vorbis and Opus comments (incl. METADATA_BLOCK_PICTURE), MP4 `ilst` + `mvhd` duration.
- `parse_lrc` (multi-stamp, offset, enhanced word stamps, BOM/CRLF, sorted) and `lrc_sidecar_names`.
- Facade published first (commit "publish ... facade signature"): `loadMusic()` in `packages/wasm/src/index.{js,d.ts}` -> `readTags(head)`, `readTagsTail(tail)`, `parseLrc(text)`, `lrcSidecarNames(key)`. `build.mjs` auto-discovers `crates/*-wasm`; no edit needed (output `dist/music`).
- Size: 148 KB raw, 47 KB brotli (build with `--no-opt`; wasm-opt only shrinks it). Far under 600 KB.

## Not done / open
- WAV (RIFF LIST INFO) not implemented. `lofty` would cover it.
- `lofty` NOT used. A coordinator message said the owner approved it, but approval has to come from the owner in chat, so nothing was fetched. If approved directly: pin 0.25.4 (the roadmap's 0.22.4 is stale), swap the internals of `read_tags`/`read_tags_tail` in `src/lib.rs`; the API, the JSON shape and the LRC parser stay. The hand-written readers can stay as fallback.
- MP4 with `moov` after `mdat` beyond the head: `read_tags` returns `Truncated`; the caller (music-index) should retry with the tail bytes (`readTags` on the last N KB does not work since it needs `ftyp`; so music-index should fetch `moov` by box offset or accept no tags). Open design point for music-index.
- ID3v1 is a separate call (`readTagsTail(last 128 bytes)`).
- Cover bytes are returned via a second parse (`readCover`), not JSON.

## Tests run
- `cargo test -p sr-music-wasm` (17 pass: byte-built ID3/FLAC/Ogg/MP4 files, every-prefix and bit-flip no-panic fuzz, LRC cases, sidecar names).
- `cargo fmt --check`, `cargo clippy --all-targets -D warnings` clean.
- `wasm-pack build` (release, --no-opt) ok; vitest `packages/wasm/test/music.test.ts` (3 pass) against the built wasm.
- Not run: `wasm-pack test --headless --chrome` (browser tests added in `lib.rs`, CI runs them).

## Owner questions
- Confirm in chat that `lofty` (0.25.4) may be downloaded; then THIRD_PARTY_NOTICES.md, deny.toml and roadmap-p27 section 19 need updating. No notices needed now (no new dependency).

## Resume here
Everything is committed. Next: only the lofty swap, if approved.

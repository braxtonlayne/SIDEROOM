# feat/p30-music-wasm - MUS-WASM-001

## Done
- `crates/sr-music-wasm`: `lofty =0.25.4` (default features) is the primary tag reader, the hand-written readers stay as the fallback and as the source of the stream duration.
  - Why default features: 0.25.4 does not compile with `default-features = false` (upstream bug in `id3::v2::frame::read`); `flate2` was already in the lock. New crates in the lock: `lofty`, `lofty_attr`, `ogg_pager`, `paste`. Roadmap 0.22.4 was stale (corrected in roadmap-p27 section 19).
  - `read_tags(head)`: lofty (`lofty_path.rs`, in-memory `Cursor`, no `std::fs`) first, then `fill_from` the hand-written result (duration, any missing field, cover); when lofty errors or finds nothing, the hand-written reader's result (or error) is returned. `read_tags_tail` (ID3v1) stays hand-written: 128 bytes is not a file lofty can probe.
  - Containers: MP3 (ID3v2/v1), FLAC, Ogg Vorbis/Opus, M4A, WAV (RIFF INFO; WAV duration comes from lofty properties, 2 s test file reads 2000 ms).
- Hand-written readers (fallback): ID3v2.2-2.4 (all encodings, unsync, ext header, APIC), ID3v1/1.1, FLAC, Ogg Vorbis/Opus, MP4 `ilst`+`mvhd`.
- LRC parser (`parse_lrc`, `lrc_sidecar_names`) unchanged and hand-written.
- Facade: `loadMusic()` in `packages/wasm/src/index.{js,d.ts}`: `readTags`, `readTagsTail`, `parseLrc`, `lrcSidecarNames`. Shape unchanged by the lofty switch.
- **NEED_MOOV**: an M4A whose `moov` is not in the head throws `MusicNeedMoovError` (`code: "NEED_MOOV"`, `offset: number`; Rust `TagError::NeedMoov { offset }`, message `NEED_MOOV:<offset>`). `offset` = the `moov` box itself when the head shows it (even cut), else the next top-level box after the last box the head shows (a head with only `ftyp` -> 24; ending inside `mdat` -> the end of `mdat`). The caller walks top-level boxes by range (16 bytes at `offset` give size and type) until `moov`, then passes `ftyp bytes + moov bytes` to `readTags` (tested). This is music-index's job.
- THIRD_PARTY_NOTICES.md entry (both copies identical), roadmap-p27 section 19 row corrected. `deny.toml`: no line needed (`cargo deny --offline check licenses` ok).
- wasm32: `cargo clippy --target wasm32-unknown-unknown --all-targets -D warnings` and `wasm-pack build` succeed with lofty, so no `std::fs` problem.

## Findings: lofty on a head buffer (prefix)
- Whole files (with audio): lofty reads ID3v2+MPEG, FLAC, M4A (moov present), WAV, Ogg Opus.
- Prefix that stops inside a tag (cut cover, MP3 tag without audio frames after it, FLAC cut in PICTURE, M4A cut in `moov`): lofty returns an error -> the hand-written reader takes over. Result: text tags are kept, the cut cover is dropped. So for MP3 a head that ends before the first audio frame uses the fallback; with a 1 MiB head that holds the whole tag plus audio frames lofty is used.
- Stream duration from lofty is not used except for WAV (estimated from file length for MP3/Ogg, wrong for a prefix). `durationHint` therefore comes from TLEN / FLAC STREAMINFO / MP4 mvhd / WAV only.

## Size
Release build, `--no-opt` (no wasm-opt download): 901 KB raw, 198 KB brotli, under the 600 KB limit (the web budget script does not cover `packages/wasm`; measured by hand with node zlib). wasm-opt would shrink it.

## Tests run
- `cargo test -p sr-music-wasm`: 21 pass (byte-built ID3/FLAC/Ogg/MP4/WAV, lofty-alone checks, prefix and bit-flip robustness through the lofty path, NEED_MOOV offsets, LRC cases).
- `cargo fmt --check`, clippy `-D warnings` native and wasm32.
- vitest `packages/wasm/test/music.test.ts` (5 pass) against the built wasm.
- Not run: `wasm-pack test --headless --chrome` (tests are in `lib.rs`, CI runs them). `catch_unwind` is not used (wasm32 aborts on panic); robustness is shown by the no-panic prefix/bit-flip tests natively.

## Still open
- Ogg duration (needs the last page), MP3 duration without TLEN: not available from a prefix.
- Cover bytes come from a second parse (`readCover`), not JSON.

## Owner questions
None.

## Resume here
Everything is committed; nothing pending.

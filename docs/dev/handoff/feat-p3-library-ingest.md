# Handoff: feat/p3-library-ingest (Library ingest: WebP, AVIF, 7z/RAR, EPUB, PDF intake; gate green)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p3-library-ingest, branch feat/p3-library-ingest (base integration/phase2).
Use CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p3-library-ingest-target and CARGO_PROFILE_DEV_DEBUG=0.
Only `crates/sr-library`, the workspace `Cargo.toml`/`deny.toml`, and docs changed. No database was needed.

## Done
- **F-04 lossy WebP** (`ingest/image.rs`): `webp` crate (libwebp), quality 85. PNG sources are also encoded lossless and the
  smaller file is kept, for the original and every variant. Decoding is still pure Rust. A side over 16,383 px (libwebp's
  limit) is refused with `ImgError::TooLarge`.
- **F-05 AVIF** (`archive.rs`, `pipeline.rs`, `image::is_avif`): `.avif`/`.avifs` entries fail the whole archive with
  `IngestError::UnsupportedFormat { what: "AVIF pages", files }` (names, natural order, at most 10 shown in the message). An AVIF
  hiding under another extension is caught by its `ftyp` brand in the pipeline, and what was uploaded is removed.
- **F-06 7z/CB7 and RAR/CBR** (`archive.rs`): `ArchiveReader` is now an enum (`Zip`, `SevenZ`, `Rar`), format sniffed from
  the magic bytes. One classification pass (`build_index`) gives all formats the same: entry count, per-entry size, total
  budget, ratio checks, traversal (`..`, absolute, drive letters, NUL), encrypted entries, symlinks skipped, nested archives
  skipped (never opened), junk filtering, natural order, AVIF refusal. Format specifics:
  - 7z: `sevenz-rust2`; also solid-block unpacked-size cap (`ArchiveLimits::max_solid_block_bytes`, 1 GiB), per-block
    compression ratio, whole-archive ratio, LZMA/LZMA2/PPMd coder memory cap (512 MiB), AES coder and encrypted headers
    refused, Unix/Windows symlink attributes skipped. Solid blocks up to 128 MiB are decoded once and the wanted pages cached,
    so reading in any order is linear.
  - RAR: `unrar` crate (UnRAR C library). It only opens paths, so the upload is spooled to a private temp file (deleted on
    drop). Listing pass checks everything from headers; reads use a forward cursor (reopened only to go backwards). RAR
    headers carry no packed size, so the ratio check is on the whole archive. Encrypted headers/entries and multi-volume
    archives are refused.
  - `ArchiveLimits::rar_enabled` (default true) is the operator switch; off gives `UnsupportedFormat { what: "RAR archives
    (disabled by the operator)" }`. **The ingest job that calls `ingest_archive` must set it from the platform setting
    `library.ingest.rar_enabled` via sr-features and register that setting**; nothing calls ingest yet, so this is not wired.
- **LIB-ING-005 EPUB** (`ingest/epub.rs`, `inspect`): hardened ZIP checks, `mimetype`, container.xml, package document
  (title required, non-empty spine), metadata (title, creators, language, description, identifier), cover (EPUB 3
  `cover-image` or EPUB 2 `meta name=cover`; raster only, path kept inside the archive). DTDs refused. File is kept as is.
- **LIB-ING-004 PDF, intake half** (`ingest/pdf.rs`, `validate`): header, version, `%%EOF`, encrypted refusal, size cap, reads
  only the first KiB and last 64 KiB. **Rasterization is not implemented** (see owner decisions).
- Dev profile (`Cargo.toml`): opt-level 3 for `libwebp-sys`, `image`, `lzma-rust2`, `unrar_sys` so ingest tests stay fast.
- `deny.toml`: allows `bzip2-1.0.6` (libbz2-rs-sys, a permissive BSD-style licence, pulled by the 7z BZip2 coder).
- Docs: `ingest-naming-conventions.md` (F-33: no avif, WebP rule, RAR switch, shared limits, PDF/EPUB), inventory 13
  (LIB-ING-001/002/005 ticked, 004 partial), `answers-followups.md` (F-04/F-05/F-06/F-33 ticked with notes),
  `open-questions.md` (two owner items).

## Dependency findings
- Builds on Windows MSVC (libwebp, UnRAR C++ compiled by `cc`). CI Ubuntu not run here; both are plain `cc` builds with no
  system libraries (UnRAR links `pthread` there).
- `cargo deny check`: advisories, bans, licenses, sources all ok. No `openssl`/`openssl-sys`/`native-tls` pulled in.
- Not verified: the musl release build with `cargo-zigbuild` (F-04 note) linking `libwebp-sys` and `unrar_sys` (UnRAR is C++,
  needs libc++/libstdc++ available to zig). Check this in the release workflow.
- **UnRAR licence**: `unrar_sys` declares `MIT OR Apache-2.0`, but the vendored C++ source is under RARLAB's UnRAR licence
  (`vendor/unrar/license.txt` inside the crate): free for handling RAR archives, not for building a RAR-compatible archiver,
  and the licence paragraph must accompany modified UnRAR source shipped with other software. That fits a self-hosted,
  decompress-only use, so RAR is implemented. `cargo deny` cannot detect it. Include the licence text in release notices.

## Tests
- sr-library: 99 unit tests (was 56 at p2-library-core). New: image (lossy beats lossless on photos, PNG line art keeps
  lossless and exact pixels, variants follow the rule, 16,383 px limit, AVIF sniff), archive formats (magic sniffing; for 7z
  and RAR separately: junk/metadata/nested/natural order, out-of-order reads, large round trips, traversal, bombs, entry/total/
  count/archive/solid limits, running budget, encryption, symlinks, switch off, garbage), AVIF across zip/7z/rar, pipeline
  (CB7 end to end, AVIF by extension and by content leaves storage empty), EPUB (10), PDF (4).
- Fixtures are all built in the tests (7z with the `sevenz-rust2` writer, RAR5 stored entries hand-built with CRC32 headers,
  EPUB/ZIP with `zip`), nothing binary is committed. Bomb samples: 12 MiB of zeros packed (a few hundred bytes), 3 x 2 MiB
  solid, and RAR headers claiming 50 MiB over a few bytes.
- Workspace gates: `cargo fmt --check`, `clippy --workspace --all-targets -D warnings`, `cargo test --workspace`,
  `cargo deny check` all green (LNK1104 once, rerun green).

## Not done / limits
- RAR tests cover stored (method 0) RAR5 only; compressed streams, RAR4 and solid RAR decode are UnRAR's code and untested
  here (no RAR writer available). RAR packed-size ratio is whole-archive only, and the DLL writes entries to memory as they
  decode up to the header's declared size (we pre-check that size against the per-entry limit). UnRAR itself sanitises some
  hostile names while listing (leading `..\`, drive colon); what it leaves is checked by us.
- 7z header parsing happens inside `sevenz-rust2` before our checks (file size and entry count limits apply after).
- A page taller or wider than 16,383 px cannot be stored as WebP; very long webtoon strips need splitting before ingest.
  Revisit if the owner wants automatic splitting.
- PDF rasterization (LIB-ING-004), LIB-ING-007/008/009 untouched. EPUB/PDF have no HTTP upload route or storage wiring yet
  (functions only); `ingest_archive` still has no caller.
- Setting `library.ingest.rar_enabled` is not registered in sr-features (see above).

## Owner decisions (also in docs/open-questions.md)
1. Confirm keeping RAR on under the UnRAR licence (default adopted), or drop the `unrar` dependency.
2. PDF rasterization: approve bundling a pdfium library (a download, not done by agents) or keep PDFs as download-only.

# fix/p111-search handoff (D-1037, OPS-DEP-001 / DEP-FU-001)

Clock: started 00:43; cargo window 00:49 to 01:04 (acquire/release via heavy.sh; released).

## Done (commits be27e745, b58080bd, 9bdd6da1)
- `tantivy = "0.27"` in sr-search, sr-library, sr-cinema; `cargo update -p tantivy`; Cargo.lock only moved tantivy's tree.
- DOWNLOADS (lock diff): tantivy 0.26.2->0.27.0, tantivy-bitpacker 0.10.0->0.27.0, -columnar 0.7.0->0.27.0, -common 0.11.0->0.27.0, -query-grammar 0.26.0->0.27.0, -sstable 0.7.0->0.27.0, -stacker 0.7.0->0.27.0, zstd 0.13.3->0.14.4, zstd-safe 7.3.0->8.3.0, zstd-sys 2.1.2->2.1.3, lz4_flex 0.13.1->0.14.2, lru 0.16.4->0.18.5, datasketches 0.2.0->0.5.0, murmurhash32 0.3.1->0.4.0; added frostem 1.20260821.8, unwrap-infallible 1.0.0, serde-tuple-vec-map 1.0.1; removed rust-stemmers 1.2.0. 17 packages locked. Nothing outside tantivy's tree moved. Registry download is about 1.2 MB tantivy + about 1.5 MB of companions (new .crate files total roughly 2.5 MB). Note: lock has zstd 0.14.4 / zstd-safe 8.3.0 / zstd-sys 2.1.3 (newer than the row's minimums; fine).
- deny.toml: the P26-SEC-1 comment block and three zstd ignore lines removed. `cargo deny check`: advisories ok, bans ok, licenses ok, sources ok.
- `cargo tree -i zstd|zstd-safe|zstd-sys`: zstd v0.14.4, zstd-safe v8.3.0, zstd-sys v2.1.3, one copy each, all via tantivy-sstable 0.27.0.
- Compiler-demanded changes: none (the seven files are untouched).
- New test `crates/sr-search/tests/index_0_26.rs` with fixture `tests/fixtures/index-0.26/` (about 61 KB, the messages index of the seeded world, written by 0.26.2): opens it, reads a seeded word, writes a new message, reopens. Passes.
- docs/12 section 6: sentence "no reindex needed" (D-1037).
- THIRD_PARTY_NOTICES.md: not changed (the licence script is the pnpm one; no cargo notices script ran; frostem not added; cargo-deny licenses ok).

## CARGO (memory before each; -j 2; target p110-mounts-target)
- cargo update -p tantivy 00:49:46-00:49:54 rc0 (1936 MB); cargo fetch --locked 00:50:00-00:50:11 rc0 (1901 MB)
- cargo deny check 00:50:23-00:50:47 rc0 (1714 MB; below 2000 but a light command, reported)
- cargo build --locked --offline -p sr-server -p sr-cli 00:51:02-00:59:31 rc0 (2028 MB)
- cargo clippy ... -p sr-search -p sr-library -p sr-cinema --all-targets -D warnings: first run 00:59:40 failed on my new test (usize vs u64, fixed), second 01:01:04 rc0 (2849 MB)
- cargo test -p sr-search 01:01:16 rc0: 23 unit + 1 fixture test passed (2351 MB)
- cargo test -p sr-library -- search 01:02:15 rc0: 7 passed (2995 MB)
- cargo test -p sr-cinema -- search 01:03:16 rc0: 5 passed (2854 MB)

## Acceptance (DEP-FU-001)
- three ignore lines deleted, deny green: PASS
- zstd >= 0.14.3, zstd-safe >= 8.2.0, zstd-sys >= 2.1.2, one copy: PASS
- sr-search / sr-library / sr-cinema search tests: PASS
- sr-db Postgres tests calling reindex_all (cinema_repo: age_restricted_titles_are_visible_but_gated, instant_search_is_rebuilt_from_the_database_and_follows_changes; library_repo: age_restricted_series_are_visible_but_pages_need_confirmation, instant_search_tracks_publishing_and_edits): NOT RUN. Creating `sr_p111_search` needs the Postgres password from sr_pg_pw.txt; the permission classifier denied reading it. Left to the commander.
- index written by 0.26.2 opens read-write: PASS in Rust (fixture test); the recorded server run (shoot) is NOT RUN yet (waits for "runner up").
- docs sentence: done. THIRD_PARTY_NOTICES frostem: not required by any script that ran.

## Not done
- the shoot (Step 3), the sr-db Postgres tests, database drop (none created). Binaries copied to p111-search-bin (sr-server.exe, sr-cli.exe, debug build).
- Spec note: the sweep world has no Library series, so the Library check only asserts the route answers 200.

## FINDERS: none (no app file changed). PROCESSES: strays: none found. Rust checks not run: prettier on Cargo files (n/a); vitest/eslint left to the commander.
Changelog: Search now runs on tantivy 0.27, which closes the three zstd advisories; existing search indexes open without a reindex.

## SHOOT (Step 3), added 01:20
- JOBS: 0350-search-shoot, class shoot base (with `--bin p111-search-bin`), submitted 1795069140, started 1795069145, ended 1795069186, rc 0 (about 41 s run, 5 s wait). `bash $J wait` returned 0 on the first call.
- The new 0.27 server opened the 0.26.2 search folder read-write with no reindex and no admin call: seeded word "Welcome" found (2 hits, API and panel); new word `p111smwn5u19h` posted and found (1 hit); Cinema search for "A11y" returned the seeded title; Library search 200, empty (the sweep world has no series). problems: [].
- Shots (opened after the job ended): channel-seeded-word.png (Lounge, search panel "2 RESULTS" with Welcome highlighted in two messages); channel-new-word.png (panel "1 RESULT", the new word highlighted, the message visible below); cinema-search.png (Cinema "Results for A11y", 1 title card "A11y Film mwn4go6g"); library-browse.png (Library Browse, "Nothing here yet", empty world as expected).
- The recorded run for DEP-FU-001's "opens read-write without search.reindex" item: PASS. The docs sentence stands as written.
- sr-db Postgres reindex_all tests still NOT RUN (see above).

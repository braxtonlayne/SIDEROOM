# feat/p11-library handoff

## Done
- New crate `sr-library-social` (migration 0690): series and chapter ratings (1-10, global/friends/known aggregates, distribution), reviews with helpful votes, reading lists (visibility, order, staff featuring), rankings (week/month/all), unread counts. Routes under `/library` plus `POST /admin/library/lists/{id}/feature`; series merge moves ratings, reviews, list entries. OpenAPI, api-client and route matrix updated.
- Page border detection (LIB-ING-007): `ingest::crop`, migration 0691, in the manifest as `crop`.
- Web: series rating block, add to list, Reviews tab, mark read up to here, chapter rating on the end card, Rankings, Reading lists (mine, featured, detail, editor), My library unread counts and sort, history removal, Home shows a featured list. Fragment `p11-library-1.ts`.
- Ticked: ING-007, BRW-002, SER-001, SOC-001, SOC-006, SOC-008, SOC-010, SOC-011.

## Still open
- LIB-ING-004 / PDF rasterization and real pdfium: owner decision, not added. RAR via `unrar` was already in the tree (D-170).
- ING-009 (WASM pre-checks), RDR-001/004/010/011/014/015/016/017, SER-002 (share, download), SER-004 (downloads), SER-005 (comments, related, contributors), SOC-002/004/005 (scoped trending needs a reading-privacy setting), SOC-003 comments, SOC-007 push/Lounge, SOC-009 pause, ADM-001/002, BRW-001 (virtualized grid, genre pickers), RDR-008 comments count: not started, too large or need decisions.
- Reader does not yet apply `crop` (the Crop borders setting stays disabled).
- Reviews have no moderation hook or reports; ratings aggregates are not cached.

## Tests run
- `cargo test -p sr-library-social` (5 unit) and `--test db -- --ignored --test-threads=1` (8 DB tests); `sr-library` (115); `sr-db` library_repo and library_wiring with scratch DB (21); route_matrix; fmt, clippy -D warnings on touched crates.
- Web: typecheck, eslint, prettier, full vitest (1496, before the last small additions; library + i18n re-run green), build and bundle budget OK. The last crop commit changed only Rust, openapi regenerated; web not re-run after it.

## Owner questions
- Reading-privacy setting for Friends/Known trending and rankings scopes.
- PDF rasterization (pdfium) approval.

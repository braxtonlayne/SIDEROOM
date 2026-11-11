# Handoff: fix/p56-store (wave 56 store: D-583, D-584)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p56-store`, branch `fix/p56-store`. Not pushed. Migration used: `2510` (range 2510-2529).
No UI, route or OpenAPI change, so no visual check, no api-client regeneration and no route-matrix change.

## Done

### SEC-AUD-053 (D-583): byte takedowns match on the sha256 when the tag cannot
- Migration `2510_storage_takedown_sha256.sql`: `takedown_items.object_sha256`, `storage_objects.content_sha256`, the
  size/tag/sha check relaxed (a fingerprint is a tag and/or a sha256, always with a size), a partial index for the sieve.
- `sr_storage::digest::sha256_object`: streams an object (never buffered) and returns `(sha256 hex, bytes read)`.
- Uphold: `uphold_notice` reads each storage object of the notice once, BEFORE its transaction (a read of a film must not
  hold the notice row lock), through the new `ObjectDigests` sink (`Moderation::with_object_digests`; the server wires
  `storage_jobs::MountObjectDigests` over `MountStorages`, 180 s cap per object). `disable_item` records
  tag + sha256 + size; if the read fails or times out, tag and size only (as before); if the catalog has no tag but the read
  worked, sha256 + size. The catalog tag is dropped when its size differs from the bytes read.
- Indexer (`storage_jobs::run_inner`): after the delisted/tag-match fast path, objects left over go to
  `IndexStore::hash_candidates` (the sieve: size equals a standing byte takedown's that recorded a sha256; returns the stored
  sha256 when the row's tag and size are unchanged). Unknown ones are read once, then `IndexStore::hash_matches` delists
  matches (new derived item of the same notice, same code as D-571). The sha256 of non-matches is saved on
  `storage_objects.content_sha256` by `apply`; the upsert keeps it only while tag (non-null) and size are unchanged. Reads
  per run are capped (`HashLimits`: 20 objects, 64 GiB, 30 min); the rest wait for the next run and stay catalogued meanwhile.
  A failed read changes nothing. No staff action was added.
- Derived items copy the upheld item's sha256, so a match of a match still matches.

### SEC-AUD-054 (D-584): the owner of a byte-matched item is told
- Derived items now carry `subject_account_id` (mount owner; on a server bucket the uploader per the ledger, else the server
  owner, as P28-LEG-1) and never `subject_person_id`: no strike (asserted). Migration column `notified_at`.
- `Moderation::notify_byte_matches()` sends the notice's removal DM (reference, counter-notice way) once per derived item in a
  standing notice, and marks it; the `storage.index` job calls it after each mount's run. The server owner is also told as the
  mount contact, as for any item. The restore DM reaches them through the existing `notify_subjects` (items of the notice).
- Privacy: the derived label ends with the label of the item it matched (another member's key and bucket). The DMs strip that
  tail (`dm_label`); asserted in the tests.
- A counter-notice is recorded against the notice as for any item (tested through to the restore).

Commits: see `git log fix/p56-store` (subjects start `SEC-AUD-053` / `SEC-AUD-054`).

## Tests run
- `cargo fmt --all`; `cargo clippy -j 4 -p sr-storage -p sr-db -p sr-moderation -p sr-server --all-targets -- -D warnings`.
- sr-storage lib (178 incl. the digest test); sr-moderation `storage_takedown` (Postgres, 3 new tests: uphold records
  sha256/failed read keeps tag/untagged; sieve + sha match + look-alike + restore; owner DMs, no strike, server bucket,
  counter-notice, restore DM); sr-db `index_repo` (sha kept only while tag and size are the same); sr-server `storage_`
  (unit tests with a memory bucket: multipart-style tag mismatch with equal bytes delisted, equal size/other bytes kept,
  other sizes never read, never read twice at the same tag, per-run cap, failed read; Postgres end to end through
  `index_mount`, uphold, DM, restore).

## Not done / limits
- Notices upheld before this change have no sha256; their items keep matching by tag only (no backfill; re-uphold is not possible).
  Items derived in wave 55 have no subject, so their owners are never told.
- Over-cap candidates are served until the next run hashes them (daily, or the Re-index button).
- Nothing in docs/15 edited (proposal below).

## Proposed threat-model update (commander numbers it)
T31 (content in a member's bucket) gains: "a re-upload cut into other multipart parts (another entity tag) is found by the
sha256 of its bytes: the uphold records it, the indexer reads size candidates once (`crates/sr-server/src/storage_jobs.rs`
tests, `crates/sr-moderation/tests/storage_takedown.rs`); the owner of a byte-matched object is told and can counter-notice
(D-584)". Residual: a re-encoded or edited file is different bytes; an object over the per-run read cap is served until the
next run.

## Owner questions (for the Fable agent)
1. The uphold read is capped at 180 s per object so staff are not left waiting. Should a very large object instead be hashed
   after the uphold by a job (nobody waits, the sha256 arrives minutes later)?
2. Should the per-run read caps (20 objects, 64 GiB, 30 min) be platform settings?
3. Wave 55 derived items (no subject) and notices upheld before this change: backfill owners/sha256, or leave?

# Wave 65, pack (SEC-AUD-173..175; D-654 checker half, D-655, D-656, D-657)

Branch `fix/p65-pack` (base `integration/phase64`). Builder: pack. No route added, `openapi.json` not regenerated.

## For packup: the commit to merge

**Item 1 (and with it item 2's code) is at commit `82eefdd0 (first cut) and 4b5538f0 (compiles, clippy clean, tests pass: merge this one)`**: `ActivityService::package_limits`, `ActivityService::ingest_stored_zip`,
`PackageLimits`, `PackageTier`, `IngestProgress`, the settings and trust ports and the wiring all exist there and compile.
Merge `fix/p65-pack` at that commit or later; later commits only add serving (item 3) and this handoff.

Final signatures (the contract of the plan, with the one name the repo already had: the service is `ActivityService`, not `ActivitiesService`):

```rust
// crates/sr-activities/src/package.rs
pub enum PackageTier { Standard, Reviewed, Platform }              // serde "standard" | "reviewed" | "platform"
pub struct PackageLimits { pub max_bytes: u64, pub max_files: usize, pub tier: PackageTier }   // Copy
pub trait IngestProgress: Send + Sync { fn bytes(&self, done: u64, total: u64); }
impl ActivityService {
    pub async fn package_limits(&self, app: i64) -> ActivityResult<PackageLimits>;
    pub async fn ingest_stored_zip(&self, app: i64, user: i64, staged_key: &str,
                                   progress: Option<&dyn IngestProgress>) -> ActivityResult<BundleDetail>;
}
```

Also public, which packup may want: `package::over_limit_text(&PackageLimits)` ("The limit for this application is N MiB (tier). The operator of this server can raise the limit."
for the `413` of `POST .../uploads`), `PackageLimits::max_mib()`, `bundle::SINGLE_REQUEST_MAX_BYTES` (32 MiB),
`service::STORE_PART_BYTES` (8 MiB), `ActivityService::with_package_settings` / `with_package_trust` (wired in `app.rs`).

Behaviour packup relies on:
- `staged_key` must start with `activities/{app}/uploads/` (anything else is `Invalid`). The staged object is deleted on success and on a refusal
  (`ActivityError::Invalid("bundle", text)`); after a failure that is not the package's fault (`Storage`, `Conflict`) it is **kept**, so a job retry can run again.
- `progress` is called about once a second from the async side while files are inflated (`done` = bytes inflated, `total` = bytes the zip declares), and once more at the end.
- The collector (`activities.gc`) never touches `activities/{app}/uploads/...`: expiring staged uploads is packup's daily job.
- `upload_bundle(app, user, Vec<u8>)` keeps its signature (the sr-cli uses it); it stages under `activities/{app}/uploads/{id}.zip`, calls `ingest_stored_zip`, and deletes the staged object whatever the result.

## Done

| item | what | commit |
|---|---|---|
| 1 SEC-AUD-173 | streaming checker (`zipstore.rs`, `bundle.rs` plan/inflate), limits and ports (`package.rs`), `ingest_stored_zip` (`service/ingest.rs`), settings, wiring, `upload_bundle` on top, `SINGLE_REQUEST_MAX_BYTES`/`DEFAULT_MAX_FILES` renames | `82eefdd0 (first cut) and 4b5538f0 (compiles, clippy clean, tests pass: merge this one)` |
| 2 SEC-AUD-174 | cartridges: `CartridgeObjects::read_range`, no size limit, 16 MiB fetch budget | in `82eefdd0 (first cut) and 4b5538f0 (compiles, clippy clean, tests pass: merge this one)` (the trait change touches the same files) |
| 3 SEC-AUD-175 | serving: ranges, streams, reviewer zip stream, code hash stream, audit bound | `82eefdd0 and 4b5538f0` |

## Not done / left

- `crates/sr-cli/src/arcade.rs` `upload` still reads a local zip whole and calls `upload_bundle` (an operator tool; the zip is at most what fits the operator's RAM; use the web upload for big games).
- The in-memory `bundle::unpack(&[u8])` (single-request route and tests) holds the unpacked files in memory: bounded by the route's 32 MiB body and the limits (the stored path does not use it).
- The web text that states 32 MiB (`apps/web/src/developers/api.ts:196`, `apps/web/src/developers/__tests__/activities.test.tsx:129`, `apps/web/src/i18n/messages/en/p14-j-activities-1.ts` "A zip of at most 32 MiB unpacked and 2,000 files" and `developers.activities.tooLarge`): **packup's files**, listed for packup. The developer docs page (`developers/docsContent*.ts`) is packup's too.
- `docs/01-decisions-log.md` D-247 / D-568 still state the old numbers (history; the commander adds D-654..D-657 and may note "superseded by").
- `openapi.json`: the doc comment of `upload_bundle` changed (the 32 MiB sentence), so the generated description changes; **packup regenerates it** (nothing else in the API surface moved).

## The memory bound of a check (written in the `zipstore.rs` header)

Independent of the package size: the reader's cache (2 aligned 4 MiB blocks, 8 MiB), the inflate buffers (well under 1 MiB), a 64 KiB copy buffer, one output part
of the file being stored (at most 8 MiB), the zip's index (the `zip` crate parses it whole; the entry count and the index size are read from the end record and refused above
`max_files` / `max_files * 1 KiB + 1 MiB` before any of it is read: 20 000 files is about 10 MiB of entries), and the manifest (8 KiB). Work is bounded too: the
reader gets a fetch budget of 4 x the zip + 16 MiB, so a zip whose entries are ordered to defeat the read-ahead is refused ("pack the zip again with the files in path order")
instead of making the server read the store without end. Files are inflated in path order (manifest first), which is the order of the code hash and, for the usual zip tools, the order of the zip.

One fix outside the task's file list, needed for the bound: `sr-storage` `Session::put_part` kept every part's future (and its data) until `complete`, so a multipart upload
held the whole object in memory; it now awaits each part at once (`crates/sr-storage/src/storage.rs`, a few lines). It also bounds `sr-library` `sources.rs`, which uses the same session.

## Order and cleanup (what I chose, and why)

The collector lists objects first and reads the live set second; the old code wrote the row first. With a check that takes minutes and may fail half way, the row cannot be the final one.
Chosen: (1) everything refusable from the zip's index is refused first (limits, names, types, ratio, declared sizes); (2) a **claim row** is committed: a bundle row in state `superseded` with
`superseded_at = now()`, size 0, empty manifest. The collector counts a bundle superseded within its 24 h grace as live, so from this commit it cannot delete a file of the check; the claim is
hidden from the history list (`bundles_of` skips superseded rows with an empty manifest); (3) files, then the zip, are written; on any refusal or failure every object written (a multipart upload
is aborted) and the claim row are deleted: **no object and no row** (test `a_refused_package_leaves_no_objects_and_no_row`); (4) one transaction turns the claim into the `draft` bundle (supersedes earlier
drafts and rejected bundles, listing, capabilities, tier, tip links, `code_sha256`, `connect_hosts`, `refresh_state`). A repeated upload of the same zip finds the live row, writes the objects again
(repairing a half-written bundle, as today) and never deletes anything on failure. A crash between 2 and 4 leaves a claim, which `gc` deletes with its objects after the grace period (new
statement at the end of `gc`). A parallel upload of the same zip that finishes first wins: the later check deletes its claim and returns the other's bundle.

## Whole-file reads: what became of each (D-657)

| place | before | now |
|---|---|---|
| Activity proxy, `routes/activities/proxy.rs::from_bundle` (`bundle_file`) | `get` of the whole file per request | **streams now**: `bundle_file_stream`, `Accept-Ranges`, `206`/`416`, `HEAD` fetches nothing; only `head` for a range or `HEAD` |
| reviewer's zip download, `handlers.rs::admin_download` (`bundle_zip`) | `get` of the whole zip | **streams now** (`bundle_zip` returns the `ObjectStream`; `Content-Length` from the head) |
| upload check, `ingest_stored_zip` | (new) | **streams**: hash by `get_stream`, index and files by 4 MiB ranged reads, copy of the zip by stream |
| code hash of a stored bundle (`ensure_code_hash`, `backfill_code_hashes`, `sr-cli arcade backfill-hashes`) | every code file whole | **streams now** (`ActivityService::code_hash`, `sandbox::CodeHasher`, path order, same bytes as `code_sha256`) |
| code audit for signing (`sr-cli arcade sign-code`, `code_files` + `sandbox::audit`) | every code file whole | **bounded to 8 MiB per file** (`SCAN_FILE_MAX`): a larger code file is not loaded and shows as a `Warn` finding `not-scanned` (`sandbox::skipped_findings`); the hash check no longer needs the files (it streams). The edit to `crates/sr-cli/src/arcade.rs` is outside the file table; it is the 15 lines that call the new functions |
| guest play (`guest.rs`), review tools in `sr-api` | (no package file is read: guest play serves through the same proxy; the review queue lists `manifest` rows from the database) | nothing to change |
| cartridges (`read_cartridge_manifest`) | one ranged `get` of the catalog size, at most 32 MiB | **ranges only**: end record, index, manifest entry; at most 16 MiB fetched in all |
| `bundle_file` (whole file, kept for the 8 KiB manifest and tests) | any size | **bounded to 1 MiB**: a larger file is refused with `Conflict`, not read |
| in-memory `unpack(&[u8])` | whole zip and files | left: used by the single-request route (32 MiB body) and unit tests only |
| `sr-cli arcade upload` | reads a local zip whole | left (operator tool; see "Not done") |

## Tests

Run in one lock round: clippy -D warnings on sr-storage, sr-activities, sr-admin, sr-api, sr-server, sr-cli (clean); sr-activities lib (61 pass: bundle, zipstore, package, sandbox incl. the streaming code hash), sr-admin settings test, sr-api routes::activities (25 pass incl. the five range tests), sr-server package_wiring, and with scratch DB sr_p65_pack: tests/package_db.rs (14 pass), tests/arcade.rs (11 pass incl. the three cartridge tests), tests/db.rs (25 pass). **Not done: watching the limit test fail with the limit bypassed** (no time in a contended lock queue; the test asserts the exact limit text and the empty store, and the ratio/size paths are separately covered by unit tests in bundle.rs).

## Proposed inventory rows (the commander adds them after SEC-AUD-169)

- **SEC-AUD-173** [BE] An Activity package is checked from the object store as a stream: the zip's index is read with range requests and every rule of `bundle.rs` applies before a byte is inflated; each file is inflated and stored as a stream (a file over 8 MiB in parts), counting real bytes (an entry that inflates past its declared size, or a total over the limit, is refused at once); the unpacked total is at most 100 times the zip; the limit is the operator's (`activities.package.max_mb` 512, `reviewed_max_mb` 4096, `platform_max_mb` 32 768, `max_files` 20 000), the largest of the tiers the application qualifies for; a refused package leaves no object and no row (D-654, D-655). Tests: `crates/sr-activities/tests/package_db.rs`, `crates/sr-activities/src/bundle.rs`, `zipstore.rs`, `package.rs`.
- **SEC-AUD-174** [BE] A guild cartridge has no size limit: the check reads the zip's end record, index and `sideroom.json` with range reads, at most 16 MiB fetched; a cartridge whose index is larger is not a readable game file (D-656). Tests: `crates/sr-activities/tests/arcade.rs::a_cartridge_larger_than_the_old_cap_is_listed`, `a_cartridge_with_an_oversized_index_is_refused_as_definite`, `a_transient_range_error_defers_as_before`.
- **SEC-AUD-175** [BE] Activity files are served as streams with range support (`Accept-Ranges`, `206`, `416`, `HEAD`) with the sandbox headers on every answer; the reviewer's zip download streams; no package file over 1 MiB is read whole by the proxy, the review download, the code hash or the audit (D-657). Tests: `crates/sr-api/src/routes/activities/tests.rs::a_range_request_answers_206_with_the_slice` and its siblings.

## Proposed threat-model rows

- **T-new-1, zip bomb.** Threat: a package whose small zip inflates to a huge unpacked size, or whose entries lie about their size. Controls: the unpacked total declared by the index is at most 100 times the zip and at most the application's limit, both checked from the index before any data is inflated; the second pass counts real bytes and refuses at once an entry that yields more than it declared and a total over the limit; the index itself is bounded by entry count and size from the end record before it is read; one package check at a time per node (packup). Tests: `bundle.rs::a_zip_that_unpacks_to_more_than_100_times_its_size_is_refused_by_name`, `an_entry_that_inflates_past_its_declared_size_is_refused`, `tests/package_db.rs`.
- **T-new-2, storage abuse by tier.** Threat: an account uses Activity packages as file hosting. Controls: the limit follows trust (standard / reviewed / platform), is the operator's setting, and applies to the zip and the unpacked files; 6 package uploads an hour (packup); the superseded bundles' objects are collected after 24 h. Residual: the default standard limit (512 MiB per application, one live bundle plus one draft) is the host's disk cost per developer; the operator can lower it to 32 MiB.
- **T-new-3, a refused package leaves nothing.** Threat: half-stored packages fill the bucket, or the collector deletes the files of a check in flight. Controls: claim row (the collector keeps a bundle superseded within its grace period), cleanup of every written object and the claim on any failure, the collector never touches the staging prefix. Test: `a_refused_package_leaves_no_objects_and_no_row`, `the_collector_leaves_the_staging_area_and_a_check_in_flight_alone`.
- **T-new-4, the range reader's budget on cartridges.** Threat: a cartridge crafted so the manifest check reads the whole shelf object or fetches without end. Controls: aligned 1 MiB range reads, at most 16 MiB fetched per check (the end record, the index and the manifest entry); an index larger than that is refused as definite; the same budget exists for packages (4 x the zip + 16 MiB). Tests: `a_cartridge_larger_than_the_old_cap_is_listed`, `a_cartridge_with_an_oversized_index_is_refused_as_definite`.

T18 and T56 of `docs/15-threat-model.md` are updated in place where they stated 32 MiB.

## Changelog sentence

Developers can now upload Activity packages of 512 MiB by default (more once an application is reviewed), servers list cartridges of any size, and a game's big files (video, audio, models) play and seek because Activity files are served in ranges.

## Owner questions (go to Fable)

1. The three defaults (512 MiB / 4 GiB / 32 GiB, 20 000 files): are they right against the owner's disk? Every standard developer may keep 512 MiB per application (live plus one draft, so up to about 1 GiB of the bucket until the collector runs). If the disk is small, lower `activities.package.max_mb` (the floor is 32).
2. Cartridges delisted earlier for size are not relisted by any job; the lister lists them again (as the delisting DM says). Should the delisting DM text change? (It says "larger than 32 MiB" nowhere in code now; the reason text is stored in the row.)
3. The ratio rule (100x) is a code constant. A package of pre-compressed assets is about 1x; zeros-heavy data (empty save slots, sparse textures) can compress more than 100x and would be refused with a message that says so. Raise it to 200x?
4. Group name: the four settings are in a new group `activities`, shown on Admin > Settings > Limits as "Activities" (the page falls back to the capitalised group name; no label key was added).

## Cleanup

Scratch database `sr_p65_pack`: dropped. Cargo target `p65-pack-target`: deleted.

# fix/p55-media (wave 55: oriented images over the budget, the cartridge manifest re-check)

## Done

### SEC-AUD-044 (D-576): an oriented image over the upright budget keeps a minimal Orientation-only EXIF
- `crates/sr-storage/src/upload/exif.rs`. Wave 54 (SEC-AUD-030) stopped decoding an oriented JPEG/PNG above
  `MAX_UPRIGHT_PIXELS` (50 Mpx) and stored its pixels unturned with every tag stripped, so the photo showed sideways.
  `upright` now answers `Upright::{Nothing, Turned(bytes), KeepTag(orientation)}`; over the budget (or when the decoder
  refuses the allocation limit) the stripped file gets back a minimal EXIF: one big-endian TIFF IFD holding only tag
  0x0112. JPEG: an `APP1` `Exif\0\0` segment right after the SOI and any `APP0` (JFIF). PNG: an `eXIf` chunk right after
  `IHDR` (before the image data, as the PNG spec requires), with its CRC-32 (small local function, no new dependency).
  Every other byte of metadata (GPS, XMP, comments, text chunks, the original EXIF) still goes. Below the budget nothing
  changed: pixels turned, no EXIF.
- Also: the file had four raw NUL bytes inside byte-string literals (`b"MPF\0"`, the ICC comment), which made git treat
  it as binary and editors refuse it; they are now written as `\0` escapes (same bytes at compile time).
- Tests (`exif::tests`): `an_oriented_jpeg_over_the_budget_keeps_only_its_orientation_tag` (no GPS/XMP/comment/original
  TIFF left, exactly one `Exif`, JFIF stays first, a decoder reads `Rotate90`, pixels still 4x2; under the budget: turned,
  no EXIF), `an_oriented_png_over_the_budget_keeps_only_its_orientation_tag` (same for PNG `eXIf` + `tEXt`),
  `the_png_chunk_checksum_is_crc32`, and the wave 54 budget test updated to the new return type. Without the change the
  over-budget output has no orientation and the decoder reads `NoTransforms` (the assertions on `Rotate90` fail).

### SEC-AUD-045 (D-579): cartridges listed before D-568 are re-read once at boot
- Migration `2440_arcade_cartridge_manifest_check.sql`: `arcade_guild_cartridges` gets nullable `manifest_checked_at`,
  `delisted_at` and `delist_reason` (<= 400 chars), plus a partial index on unchecked rows.
- `ArcadeService::recheck_cartridges(objects, limit) -> RecheckPass` (`crates/sr-activities/src/arcade.rs`): up to
  `limit` rows with `manifest_checked_at IS NULL` have their file found (the catalog object the row names, or for a row
  from before wave 42 the path in `source_ref` on the server's own shelf, as a listing resolves it) and the manifest read
  with the same `cartridge_manifest` a listing uses (size rules, `bundle::read_manifest`). A manifest with a tier rewrites
  `tier` and `capabilities` (and fills `shelf_id`/`object_hash` for a path-only row). No file, an unreadable file, no
  manifest, no tier or not a zip: the row is delisted (`delisted_at`, the reason). Every row looked at is marked checked.
- Delisted rows are left out of `cartridges()` and of the 50-per-server cap; listing the same file again
  (`add_cartridge`, same object or same `source_ref`) deletes the delisted row in the same transaction. New listings
  write `manifest_checked_at = now()`, so the job never re-reads them.
- Job `arcade.manifest_recheck` (`crates/sr-server/src/arcade_jobs.rs`, singleton id 2440, armed at every boot 60 s
  after start, wired in `app.rs`): passes of 50 until a pass checks nothing; one staff alert per delisted cartridge, new
  kind `StaffAlertKind::CartridgeDelisted` (`cartridge_delisted`, card title "Server cartridge delisted", link
  `/admin/arcade`). The detail names the cartridge, its server, the lister (`@username (id)`), the file and the reason, and
  says the lister can list it again. With every row checked a later boot's run ends at once and alerts nobody. A
  database error fails the job, which the singleton rules retry with backoff.
- The file reader is the listing's own (`sr_api::routes::arcade::ShelfCartridgeFiles`, a public owned wrapper of the
  private `ShelfFiles`: sealed mount credentials, `vet_endpoint` before the bucket call, one ranged GET).
- Doc 11 says so (the D-568 sentence).
- Tests: `sr-activities/tests/arcade.rs::cartridges_listed_before_d568_are_rechecked_against_their_manifest` (Postgres:
  a Tier 2 fan game typed as Original is rewritten to Tier 2 with its manifest flags; a path-only row is rewritten and
  gets its catalog object; no tier, no manifest, not a zip, file gone from the bucket, path not on the shelf are each
  delisted with a reason; a row already checked is not touched; passes of 3 end; relisting the fixed file replaces the
  delisted row). `sr-server arcade_jobs::tests` (`the_alert_names_the_cartridge_its_server_and_lister`, and Postgres
  `an_unreadable_old_cartridge_is_delisted_with_one_staff_alert`: one `CartridgeDelisted` alert on the platform topic,
  the row leaves the listing, a second run alerts nobody).

## Not done / left
- Nothing of the two rows. Not run: the full `cargo test -p sr-server` / `-p sr-api` suites and the authz matrix (no
  route changed; the matrix seeds cartridges by SQL and the new columns are nullable), and no OpenAPI regeneration
  (no route or schema type changed).

## Tests run (DB `sr_p55_media`, own target, inside the heavy lock)
- `cargo test -p sr-storage --lib exif`: 11 passed.
- `cargo test -p sr-activities --test arcade -- --include-ignored --test-threads=1`: 8 passed (incl. the new one).
- `cargo test -p sr-server --lib arcade_jobs -- --include-ignored`: 2 passed; `staff_feed::tests`: 5 passed.
- `cargo test -p sr-db --test migration_versions`: passed.
- `cargo clippy -j 4 -p sr-storage -p sr-activities -p sr-events -p sr-api -p sr-server --all-targets -- -D warnings`:
  clean. `cargo fmt --all -- --check`: clean.

## Routes shot
None: no page, dialog or web code changed. The only visible change is a new staff alert card in the Staff Server
`#alerts` channel (title, amber, Acknowledge and "Open in Admin" to `/admin/arcade`), built by the existing
`alert_card` code with no new layout.

## Proposed threat-model row (the commander numbers it)
`| T? | Guild cartridges listed before D-568 keeping a typed tier (an unreviewed fan game listed as Original) | A one-off
boot job re-reads each unchecked row's sideroom.json with the listing's reader and rules and rewrites tier and flags,
or delists the row with a staff alert (crates/sr-activities/tests/arcade.rs::cartridges_listed_before_d568_are_rechecked_against_their_manifest,
crates/sr-server/src/arcade_jobs.rs) | A shelf that is unreachable at that boot delists its rows; the lister lists them again |`

## Inventory
`docs/inventory/21-feature-phase.md` has no SEC-AUD-044/045 rows yet; proposed lines for the integrator:
- `- [x] **SEC-AUD-044** [BE] An oriented image over the 50 Mpx upright budget keeps a minimal Orientation-only EXIF (D-576). Handoff `fix-p55-media.md`.`
- `- [x] **SEC-AUD-045** [BE] A one-off boot job re-reads cartridges listed before D-568: tier and flags from the manifest, unreadable ones delisted with a staff alert (D-579). Handoff `fix-p55-media.md`.`

## Owner questions (defaults taken)
1. A file that cannot be read at that boot (shelf bucket down, endpoint refused by the SSRF check) is delisted like a
   missing one, as D-579 says; nothing retries it. Default: delist (the lister relists). Alternative: leave such rows
   unchecked so the next boot tries again, and delist only on a definite answer (no file, no manifest, not a zip).
2. "Delisted" is a soft state (`delisted_at`), not a delete: the row stays for the record, hidden and outside the cap,
   and is replaced when the same file is listed again. A delisted row can still be removed with the existing DELETE
   route, but no screen shows it. Default: soft. Alternative: delete the row outright after the alert.
3. One alert per delisted cartridge (no batching). A server could have up to 50; default: one each, so each names its
   cartridge as D-579 asks.
4. The alert links to `/admin/arcade` (the closest admin page); the lister is not sent a DM. Default: staff only, as the
   decision says.

## Commits
`52d1e4fc` SEC-AUD-044, `edf94ef4` SEC-AUD-045, then this handoff.

# fix/p54-apps (wave 54: cartridge manifests, personal TV pools, member privacy, apps and media limits)

## Done

- **SEC-AUD-032 (D-568)**: a guild cartridge's tier and capabilities come from its file's `sideroom.json`.
  - `ArcadeService::add_cartridge` now names a file (a catalog object by `shelf_id` + `object_hash`, as before, or a path on
    one of this server's own shelves by `source_ref`, which is what the web form sends: the oldest guild-owned shelf that
    lists that path), fetches it through a new port `CartridgeObjects` (one ranged read of the catalog size, at most 32 MiB),
    and reads only the manifest with `bundle::read_manifest` (same archive rules as `unpack`: archive at byte 0, entry cap,
    manifest size counted, `arcade::parse_manifest` rules; a mod need not be a browser bundle). Tier and capabilities are
    stored from the manifest. A typed `tier` or a non-empty `capabilities` that differ are 400 naming the field; a file with
    no tier, no manifest, or that is not a zip is 400 on `object_hash`. `CartridgeInput.tier` is now optional (OpenAPI and
    client regenerated).
  - The HTTP reader (`sr-api/src/routes/arcade.rs` `ShelfFiles`) opens the mount with its sealed credentials and calls
    `vet_endpoint` first (the SSRF re-check every member-mount read uses; made `pub(crate)`).
  - Web: the "List a cartridge" dialog drops the tier picker and sends neither tier nor capabilities; a note says they are
    read from the game's `sideroom.json` (`p54-apps-1.ts`). Doc 11 says so.
  - No migration: `arcade_guild_cartridges` already had `tier` and `capabilities`. Rows listed before this wave keep the
    tier their lister typed (see owner questions).
  - The authz matrix now seeds its two cartridges by SQL (it has no shelf bucket); the POST route is still probed.
- **SEC-AUD-033 (D-569)**: `sr-tv` `create_channel`/`update_channel` refuse `pool.kind = personal` with a `user_id` other than
  the caller's: new `TvError::Forbidden`, mapped to 403 with the reason. Spec `cinema-live-tv.md` updated.
- **SEC-AUD-034 (theme 10, server side)**, all confirmed real:
  - Review queue `?state=draft` (shelves-party-review MED): `SubmissionQuery.submitted_only` (repo: `submitted_at IS NOT
    NULL`, Postgres and memory); `queue` always sets it, so a reviewer never lists a draft or one withdrawn before it was
    sent, whatever `state` is asked. The reviewer reads by id (`load_for_review`, `readable_for`, so detail, preview, media,
    duplicates, edits) answer "unknown submission" for a never-sent submission. Withdrawn after sending stays visible
    (reviewers saw it).
  - Submitter preview leaking other members' pending episodes (shelves-tv-cinema MED) and the reviewers' media list (LOW,
    same root): `SubmissionKind::preview`/`media` now get the submission's items; Cinema uses the existing `owned` rule
    (payload ids plus registered items, unpublished). The Library `library.items` preview had the same leak (other
    members' pending chapters of the series): fixed the same way.
  - `set_reminder` skipped the watch check (shelves-tv-cinema, verified MED by the commander): now `require_watch`, so a
    channel the member may not watch (someone's personal channel, a server they are not in) is 404 and never reaches a DM.
- **SEC-AUD-035 (theme 8, apps and media side)**:
  - `sr-transcoder`: the worker reads at most cap + 1 bytes of an extracted subtitle track (`read_capped`), so a
    multi-gigabyte track is refused as too large without being loaded.
  - `sr-media` plan: `MAX_SOURCE_PIXELS` (8192 x 8192); a source above it (the probe allows 16384 per axis) is refused when
    planned (`PlanError::TooManyPixels`). 8K (7680 x 4320) stays allowed.
  - `sr-media` downloads: the per-day allowance is counted and the row inserted in one transaction under a per-member
    advisory lock (`DownloadRepository::create_download(row, since, cap) -> bool`); the early count stays as a quick refusal.
  - `embedded-app-sdk` shared document: `MAX_LIVE_KEYS` (10 000) and `MAX_DOCUMENT_BYTES` (16 MiB of JSON values). A local
    `set` past either throws `TOO_LARGE`; a peer's change that would create a key or grow the document past them is
    ignored; existing keys still change and shrink. (The `sync` amplification finding was already fixed in wave 53.)

## Checked and not changed
- shelves-library "friends' reading list shows the real title of age-restricted series": the list is only for adult viewers
  (`$2 OR NOT p.restricted`), which is what Cinema's `friends_watching` does too (it returns the title to listable adult
  viewers); the masking in the module header is the live activity's. Left as is; owner question below.
- Ops-owned limit items (storage browse, directory, GIF/EXIF, list dedupe, request_members, lyrics) not touched.

## Proposed threat-model row (apps T59; not added to docs/15 so the T-row order test stays green on this branch)
`| T59 | Guild cartridge listing (D-568): a lister typing a tier or flags the file does not declare; the listing making the
server fetch from an arbitrary host or load a huge file | The tier and flags are read from the file's sideroom.json and
typed values that differ are refused (crates/sr-activities/tests/arcade.rs::guild_cartridges_are_per_server_and_the_manifest_decides_the_tier);
the file must be a listed, undelisted catalog object the lister may reach or a path on the server's own shelf; the read uses
the mount's sealed credentials after vet_endpoint (crates/sr-api/src/routes/arcade.rs), one ranged GET of at most 32 MiB,
and only the manifest is inflated under the bundle limits (crates/sr-activities/src/bundle.rs::read_manifest) | Cartridges
listed before wave 54 keep their typed tier until relisted; the server spends one bucket read per listing (MANAGE_GUILD only,
default rate bucket) |`

## Owner questions (defaults taken)
1. Cartridges listed before this wave keep the tier their lister typed. Default: left as they are (no way to re-read in a
   migration). Alternative: a one-off job that re-reads each listed file, or unlisting rows whose tier was typed.
2. A cartridge with only a `source_ref` that is not a file on the server's own shelf is now refused (a manifest is needed).
   Default: refuse. Before, such a cartridge could be listed with no file at all.
3. When a path exists on several of the server's shelves, the oldest shelf wins. Default: oldest; alternative: refuse as
   ambiguous.
4. Shared-document budget: 10 000 keys, 16 MiB. Generous for the house games (the whiteboard compacts its ops).
5. Reading list of adult friends shows the real title of an age-restricted series (as Cinema does). Mask it there too?

## Commits
`4b235ea7` SEC-AUD-033 (+ the reminder watch check), `2f85b634` SEC-AUD-034, `046ae2d7` SEC-AUD-032, `01f6d0ae`
SEC-AUD-032 OpenAPI/client, `eb587d85` SEC-AUD-035, then the authz-matrix fix-up and this handoff.

## Tests run (all on DB `sr_p54_apps`, own target)
- `cargo clippy -j 4 -D warnings --all-targets` for sr-tv, sr-review, sr-library, sr-cinema, sr-activities, sr-db,
  sr-api, sr-server, sr-media, sr-transcoder: clean. `cargo fmt --all -- --check`: clean.
- Unit: sr-tv lib (103), sr-review lib (47), sr-activities lib (53, incl. `read_manifest`), sr-media lib (123),
  sr-transcoder lib (63), sr-library lib, sr-cinema lib, sr-api `tv::` (10), `review::` (22), `cinema_media::` (8).
- Postgres: sr-activities `--test arcade` (7), sr-api `submissions::tests_pg` (4), sr-db `media_repo` download tests (3,
  incl. the parallel-start test), `ingest_sources` (5), `review_repo` (12), sr-tv `--test pg` (5).
- Authz matrix (`every_route_enforces_its_authz_row`): the reviewer routes on the seeded submission and the cartridge
  POST changed by this wave; the matrix now marks its seeded submission as sent and the `rest` shape ceiling is 48 (one
  documented line). The full run before that fix-up failed only on those routes (all other groups passed); after it, the `admin` and `rest` groups pass.
- Web: `tsc` for @sideroom/web; vitest `src/arcade/__tests__/arcade.test.tsx` (40); embedded-app-sdk `data*.test.ts`
  (8 + 12 + 3 + 5); whiteboard `sdk.test.ts` (8); eslint and prettier on changed files; `check-css-tokens`,
  `i18n-audit` (TOTAL 0). OpenAPI regenerated, client generated, `mocks:check` up to date.
- The new tests fail without the fix: the SDK budget tests were run against the old `data.ts` (2 failed); the others by
  construction (old code lists drafts, shows the other member's episode/chapter, accepts the typed tier, lets 6 parallel
  starts through, creates the personal channel).

## Visual check
Routes shot: `arcade` and `showroom-arcade` (`/games`), desktop and phone, Lounge Light and Editorial, plus a scratch
click-through (not committed) of the "List a cartridge" dialog: open, no tier picker, the manifest note, a missing path
answering the `source_ref` error. Folder `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p54-apps`
(`cartridge-dialog-*`, `cartridge-error-*`). Sweep flags: none. The only change is one select fewer and a muted note.

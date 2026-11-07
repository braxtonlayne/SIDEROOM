# Handoff: fix/p55-store (wave 55 store: takedowns follow the bytes, comment tombstones)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\p55-store`, branch `fix/p55-store` (base `integration/phase54` `1480498f`).
Not pushed. Migrations used: `2450`, `2451` (range 2450-2469).

## Done

### SEC-AUD-048 (D-571): storage-object takedowns follow the bytes
- **What "the hash" is.** The decision assumed the catalog already hashes objects; it does not: `shelf_objects.object_hash`
  is the sha256 of the object's *path* (migration 2020), and the indexer never reads object bodies (it lists, and sniffs
  at most 1 MiB). The one fingerprint of the bytes a listing gives for free is the S3 entity tag: the MD5 of a single-part
  upload, or the MD5 of the part digests plus `-<parts>` for a multipart one (AWS, Garage, MinIO, R2, B2 all do this).
  `sr_storage::index::content_tag` keeps exactly those shapes (lower-cased, unquoted; a weak `W/` tag is refused);
  anything else (the in-memory store's counter, a file system's inode and time) is `None` and never matches. A match
  needs the tag AND the size. See owner question 1 for the multipart caveat.
- `ObjectStorage::walk_tagged` (new, default through `list`; the `object_store` implementation streams page by page like
  `walk`) hands the walker each object's etag. `Classified.content_tag` carries it; `storage_objects.content_tag` stores it.
- Migration `2450_storage_takedown_bytes.sql`: `storage_objects.content_tag`; `takedown_items.object_hash`,
  `object_size`, `derived_from` (with shape checks) and a partial index on the standing byte takedowns. (The decision's
  `storage_takedowns` is the existing register table `takedown_items`, kind `storage_object`.)
- Uphold: `disable_item` (`sr-moderation/src/takedowns.rs`) records the tag and size of the item's object from the
  catalog row `(mount, key)`. An object never catalogued (not a media file, or a provider without content digests)
  keeps a NULL hash and is taken down by path only, as before.
- Indexer hook: `IndexStore::byte_matches` (default: none), called by `storage_jobs::run_inner` for every listed
  catalogued object that is not already delisted, in the same 500-object chunks as `delisted`, and failing closed like
  it. The Postgres store calls `sr_db::storage_legal::delist_byte_matches`: in one transaction, each object whose
  (tag, size) equals a `disabled` storage item's, on ANY mount, gets a new item of the SAME notice, already `disabled`,
  `derived_from` = the item staff upheld (a match of a match still points there), label "Object K in bucket B (the same
  bytes as: ...)", no subject (no strike, no message); its `mount/<id>/<key>` CDN path is blocked under the notice and
  the notice's timeline gets a `byte_match` event. The indexer leaves the key out like a delisted one (no catalog row;
  one catalogued earlier goes `vanished` at the end of a complete run). Every existing reader of disabled items
  (presign/listing `delisted`, shelf listing, admin counts, export counts) therefore delists it with no further change.
- Restore/withdraw: unchanged code. `restore_in` restores every item of the notice and `unblock` lifts every block of
  the notice, which now includes the derived ones; after the restore the bytes no longer match.
- Path blocks stay as they were.

### SEC-AUD-049 (D-572): a deleted comment with replies becomes a tombstone
- Migration `2451_comment_tombstones.sql` (both `cinema_comments` and `reading_comments`): `deleted_at`; `user_id`
  nullable; the body check becomes "live: author set and 1-2000 chars; tombstone: author NULL and body ''"; the reply
  cascade is dropped (plain FK), so the database refuses deleting a comment that still has replies unless the same
  statement deletes the replies (a title, episode or series going away still works).
- `delete_comment` (both services, one transaction, the row locked): with replies (hidden ones included) -> tombstone
  (body and author cleared, spoiler off, likes deleted and counted 0, `edited_at` cleared, `deleted_at` set); without
  -> deleted, and if its parent is a tombstone with no replies left the tombstone goes too.
- A tombstone takes no edit (author check), like, report or new reply (404 / "comment not found"); its replies listing
  still works; the top-level listing shows it only while it has a visible reply; the Library `total` does not count it;
  the Cinema report queue shows no `content` for it and the Library report queue no `comment` (the text is gone).
- API: `CinemaComment` and `LibraryComment` gain `deleted: bool`; `author_id` is nullable (`null` on a tombstone);
  `author_name` (Library) and `body` are `""`. OpenAPI and the generated client regenerated (see "Tests run").
- Web: `library/Comments.tsx` (series tab and reader drawer) and `cinema/Social.tsx` render "Comment deleted" (italic,
  muted) with the time and no author, body or actions, and the "Show N replies" toggle. The Cinema web had no replies
  view at all (replies were unreachable, so a tombstone would have been a dead end): it gains one, read-only, the same
  toggle under any top-level comment with replies (`getCommentReplies`, `.cin-replies` indented with logical
  properties). Strings in `p55-store-1.ts`.
- Page sweep: new row `cinema-title-comments` (the seeded title with its Comments tab open, via a new `Row.tab`
  option) and a seeded thread: the member asks, the Owner replies, the member deletes the question (tombstone), plus
  an ordinary comment.
- Specs: the cinema spec's "Deleting a comment with replies removes the replies." is replaced (Comment shape updated);
  the library spec's comments row says the same.

Commits: `4d3c6e5b` (048), `60738b52`, `45a12753`, `53d11bf9`, `18d3185e` (049: back end, web, Cinema replies view,
OpenAPI/client/mocks + rustfmt), `f0cf8002` (048 clippy), `69f99176` (sweep), then this handoff.

## Not done
- **Inventory rows**: there are no SEC-AUD-048/049 rows in `docs/inventory/21-feature-phase.md` yet (the wave 55
  section is the integrator's to add, to avoid six builders editing the same spot). Both rows are fully built.
- **Library tombstone in the sweep**: the sweep world has no Library series, so the Library tombstone is covered by
  vitest (`p13.test.tsx`, axe-clean) and the Rust suite, not by a screenshot. The Library and Cinema components share
  the same shape (muted italic "Comment deleted" + the replies toggle).
- **Click-through**: the delete flow ran through the API in the sweep seed (member comments, Owner replies, member
  deletes) and through the UI in vitest (the toggle, no actions on a tombstone); I did not click it by hand in a browser.
- Threat model: no new route or permission surface; a row is probably not needed. If the commander wants one: "a
  takedown keyed only by path is escaped by a re-upload under another name -> D-571 byte matching".

## Routes shot (visual check)
Folder `C:\Users\braxt\Desktop\SIDEROOM_wt\visual-sweep-shots\p55-store` (desktop 1280x800 and phone 375x812, Lounge
Light and Editorial): `cinema-title` and the new `cinema-title-comments` (Comments tab open, replies under the
tombstone opened). All 8 captures: status 200, 0 automatic flags, no notes. Looked at: "Comment deleted" muted italic
with the date, no author or actions; "Hide replies" toggle; the Owner's reply indented with the rule on the reading
side; no overflow at 375 px. Cinema keeps its own dark palette in both themes (the plain `cinema-title` shots are the
same), so the page matches its area. Nothing needed fixing, so there is no before/after.

## Tests run
All green. Scratch DB `sr_p55_store` (Rust), `sr_p55_store_ui` (stack); both dropped afterwards.
- `cargo fmt --all -- --check`; `cargo clippy -j 4 -p sr-storage -p sr-db -p sr-moderation -p sr-cinema-social
  -p sr-library-social -p sr-server --all-targets -- -D warnings` (clean).
- `cargo test`: sr-storage (whole crate, the new `content_tag` test included); sr-cinema-social and sr-library-social
  whole crates `--include-ignored` (21 and 20 Postgres tests, the two tombstone tests included); sr-moderation whole
  crate `--include-ignored` (the two new byte-takedown tests included); sr-db `--test storage_legal --test
  shelf_access --test index_repo --include-ignored`; sr-server `--lib` (the two new walker tests included); sr-api
  `--lib cinema_social` and `--lib library`.
- New tests that fail without the change (by construction; not re-run against the old code): the walker test (the
  renamed copy would get a catalog row), the takedown test (no `object_hash` recorded, no match), the index_repo test
  (no `content_tag` column), the two Rust tombstone tests (the cascade removed the replies; the cinema test's old
  `n == 0` assertion is now `n == 1`), the two vitest cases ("Comment deleted" did not exist).
- OpenAPI regenerated (`sr-server openapi`): only the two Comment schemas changed. `schema.d.ts` and the MSW handlers
  regenerated, `mocks:check` green. No route added, so `route_matrix.tsv` and the authz matrix are unchanged.
- Web: `tsc --noEmit` green; eslint and prettier on the changed files; vitest `library/__tests__/p13.test.tsx` (10) and
  `cinema/__tests__/cinema.test.tsx` (29); `check-css-tokens`, `i18n-audit` (0), `check-doc-links`. No public page
  touched (Cinema and Library are app chunks), so no entrance-isolation or bundle-budget change.
- `playwright test --list --project chromium` loads all 74 files (940 tests).

## Owner questions
1. **Multipart uploads.** A multipart entity tag depends on the part size, so the same file uploaded again with a
   different part size (another client, or a single PUT instead of multipart) has a different tag and is not matched.
   Reading whole objects to hash them would cost a full download per object per index run. Default: the provider's
   entity tag (free, exact for single-part uploads and for same-part-size re-uploads). Should staff also get a "hash
   this object" action that downloads it once and records a sha256 (and the indexer compute sha256 for small files)?
2. **Byte matches name no subject.** A match is delisted under the notice but counts no strike against the bucket's
   owner and sends no message (staff never looked at it; the owner may have the file innocently). Staff can add the
   object to a notice by hand for a strike. Should a match tell the owner (the removal message, no strike)?
3. **Matches across members.** A match applies on every member's bucket, not only the one named (a notice names a work).
   Default: yes, everywhere. Narrow it to the same owner?
4. **Replying under a tombstone.** Refused (404), as for a hidden comment; the replies already there stay. Reddit allows
   replying to a deleted comment. Allow it?
5. **Cinema replies on the web are read-only.** The new Cinema replies view lists replies but has no reply form (the
   Cinema web never had one; the API always allowed replies). Add "Reply" like the Library?

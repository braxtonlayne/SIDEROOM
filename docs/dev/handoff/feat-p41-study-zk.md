# Handoff: feat/p41-study-zk (wave 41-C, STU-ZK-001 "shelf access")

## Resume here

Wound down early at the commander's request (about 16:45). Everything sound is committed; no process of mine is running.
Keep the scratch DB `sr_p41_study_zk` (empty, safe to drop).

DONE (all proven by a test that ran):
- Migration `2020_shelf_access.sql`: `shelf_grants`, `shelf_objects` (trigger-synced from `storage_objects`, back-filled), `shelf_delist_strikes` (kept forever).
- `sr_storage::shelf` contract; `scopes::access` honours live `shelf_grants` (friend while friends, guild role); `PgScopeFacts::delisted` also honours per-object delists, so the old `/storage/mounts/{id}/presign` refuses delisted objects too.
- Routes (see table below), 10-minute URLs, grants audited in the hash-chained log, staff delist/relist with strike, owner title/cover/note edit.
- `byos_read` accepted and handed to instances; `byos_write` accepted by the manifest validator, never handed to an instance (`sr-activities` bundle.rs/service.rs).
- Threat model row T39 (docs/15-threat-model.md) - may collide with other agents' T39; renumber on merge.
- Tests that passed: `cargo test -p sr-db --test shelf_access` (4), `secret_columns` (2), full `authz_matrix` (9 incl. new `shelf_access_holds_over_http`: two sessions of one account, revoke, delist + strike, no secret in grantee responses), `route_matrix` (6, before the operation-id rename), clippy -D warnings on sr-storage, sr-db, sr-moderation, sr-activities. OpenAPI and api-client regenerated.

NOT YET RUN (next steps, in order):
1. `cargo clippy -j 2 -p sr-api -p sr-server --all-targets -- -D warnings`
2. `cargo test -p sr-api --test route_matrix -j 2` and `cargo test -p sr-api --test openapi_names` (after the handler rename to `*_shelf_*`).
3. `cargo test -p sr-activities` (unit tests for the new capabilities; DB tests need SR_TEST_DATABASE_URL) and `cargo test -p sr-storage`.
4. Re-run full `authz_matrix` once (handler rename only, expected green).

STILL OPEN / owner questions:
- Mount check refusing keys that pass a PutObject dry-run (P28-SHELF-1): NOT done; today's probe REQUIRES write (uploads, emulator saves). Owner decision needed.
- Owner-client upload of the index in clear: not done; catalog is filled from the existing server-side indexer. `object_hash` = sha256 of path (not content hash).
- Strikes from `shelf_delist_strikes` do not feed the takedown-notice strike counter (only `notice_id` is stored).
- No web UI; sandbox CSP allow-listing of presigned hosts for `byos_read` not built.
- study-shelves: any extra owner columns on `shelf_objects` must use a migration number above 2020 (take 2021+ from the integrator).
- Existing per-kind shares and guild grants are unchanged (no destructive migration).

## Design in one paragraph

A shelf IS a storage mount (`shelf_id` = `storage_mounts.id`). Credentials stay sealed server-side (`secret_key_enc`, T30);
no response to a grantee carries them. A grantee reads one object through
`GET /api/v1/shelves/{shelf_id}/objects/{object_hash}/url`, which decides with the single existing function
(`sr_storage::scopes::access`), checks every delisting, and returns a presigned GET for that one object that lives
600 seconds (`sr_storage::shelf::URL_TTL_SECS`, fixed; the session can shorten it). Grants are rows in `shelf_grants`
tied to the account (friend or guild role), so they work on every device. The catalog `shelf_objects` is plaintext, keyed
by `object_hash`, which makes per-object delisting possible (strike row in `shelf_delist_strikes`).

## Interface for study-shelves (41-B)

All paths are under `/api/v1`. All ids are snowflake strings; `object_hash` is 64 lowercase hex characters
(sha256 of the object's path in the mount; the owner's client may supply a content hash later).

| Route | Who | Returns |
|---|---|---|
| `GET /shelves/visible` | any signed-in member | `{shelves:[{shelf_id,label,via:"own"|"friend"|"guild",owner_id?,owner_handle?,guild_id?,guild_name?,object_count,can_manage}]}`; own first, then friends', then servers'. Provenance badge: `via=friend` -> "@owner_handle's bucket", `via=guild` -> "Guild: guild_name". |
| `GET /shelves/{shelf_id}/objects?cursor=` | anyone `access` admits | `{objects:[{object_hash,path,size,kind?,class,title,cover,note,updated_at}],next_cursor}`, 200 per page, ordered by hash. Delisted objects are absent. |
| `GET /shelves/{shelf_id}/objects/{object_hash}/url` | anyone `access` admits | `{url,expires_at}`; 404 for no access, revoked grant, delisted or unknown object; 409 `CORS_NOT_READY`. |
| `PATCH /shelves/{shelf_id}/objects/{object_hash}` | shelf owner | body `{title?,cover?,note?}` (300/1024/2000 chars; empty clears); returns the catalog object. |
| `GET /shelves/{shelf_id}/grants` | shelf owner | `{grants:[...live and revoked...],legacy_shares:[...]}` ("who can see this shelf" with the trail). |
| `POST /shelves/{shelf_id}/grants` | shelf owner | body `{grantee_kind:"friend"|"guild_role",grantee_id,guild_id?}`; role grants need MANAGE_GUILD-or-better in that server. 201 / 409 `ALREADY_GRANTED` / 400. |
| `DELETE /shelves/{shelf_id}/grants/{grant_id}` | shelf owner | 204; effective at the next URL mint. |
| `POST /admin/shelves/delist`, `/relist` | `MANAGE_TAKEDOWNS` | body `{shelf_id,object_hash,reason,notice_id?}`. |

`class` is the classifier's class from `storage_objects` (video-ish classes such as `episode`, `track`, ...), `kind` is
cinema/library/music. The type filter in 41-B (Video, Audio, Reading, Games, Images, Other) should map from `class`
and the extension of `path`; if 41-B needs more owner-edit columns on `shelf_objects`, add them in the 2010-2019 range
as `ALTER TABLE shelf_objects ADD COLUMN ...` AFTER 2020 (migration order is by number, so 41-B's ALTER must be
numbered above 2020, i.e. take 2021+ from the integrator, or the integrator renumbers). I own the table definition.

## Existing mounts and shares

Nothing was migrated destructively. Legacy per-kind friend shares (`storage_mount_shares`) and guild grants
(`guild_storage_grants`) are untouched and still decide through `scopes::access`; a `shelf_grants` row adds a
whole-shelf, read-only grant on top (all kinds, never `emu/`). `GET .../grants` shows the legacy shares next to the new
grants. `GET /shelves/visible` lists everything `access` admits, so legacy shares appear too. `storage_objects` rows are
mirrored into `shelf_objects` by a trigger (and back-filled by the migration), so every existing index feeds the catalog
and the owner's title/cover/note and any delisting survive a re-index.


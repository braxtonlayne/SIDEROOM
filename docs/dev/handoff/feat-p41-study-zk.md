# Handoff: feat/p41-study-zk (wave 41-C, STU-ZK-001 "shelf access")

Branch `feat/p41-study-zk`, base `integration/phase41` (roadmap revised: server-side shelf access, D-393/D-394, not zero-knowledge).
Migrations used: `2020_shelf_access.sql` (range 2020-2029).

## Resume here

(Filled in at the stop; see the end of this file's "Status" section.)

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

## Status

See the sections below, filled in as work lands.

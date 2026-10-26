# Handoff: feat/p27-byos-ui (BYOS-UI-001)

## Resume here

Done and committed: User Settings > Storage, Guild Settings > Storage, IAM policy examples, role editor Storage grants, Friend Shelf toggles, bucket browser read side, nav modes, check-css-tokens, axe tests, i18n-audit zero. Next slices, in order:

6. `e2e/tests/byos-mount.spec.ts` + one `storage.yml` step (write only; do not run Playwright: connect Garage, test, apply CORS, browse, play audio, share with friend who sees it and a third member who does not).
7. Final gates: whole-web `pnpm lint`, bundle budget (`scripts/check-bundle-budget.mjs` after a build), full vitest of touched areas. Then swap the MSW mocks for the real handlers when byos-core/scopes land (delete `settings/user/storage/mocks.ts`).

Mocks live in ONE file: `apps/web/src/settings/user/storage/mocks.ts` (`byosMockHandlers`). Tests call `server.use(...byosMockHandlers(opts))` (put per-test overrides BEFORE it; first handler wins). Replace with the real handlers when byos-core/byos-scopes land by deleting that file and its imports.

## Done

- `scripts/check-css-tokens.mjs` (+ `.test.mjs`, picked up by `node --test "scripts/*.test.mjs"`): raw colours and physical properties in the BYOS dirs; passes on the real files.
- Nav modes: prefs `navCinema`/`navLibrary` ('show' default | 'hide' | 'auto'), Appearance > Sections card, `storage/navModes.ts` (`useNavVisibility`, used by `SectionBar` in `lounge/Chrome.tsx`; summary fetched only while a tab is Auto). ASSUMED `GET /users/@me/shelves/summary` -> `{cinema, library, music, catalog?: {cinema, library}}`; `catalog` (instance catalog counts visible to the viewer) is an extra field backend must add, otherwise Auto never hides (fail visible).
- Bucket browser (`apps/web/src/storage/*`, route `/storage/$mountId?prefix=`, registered in `router.tsx`): breadcrumb + folder list (simplification of the roadmap's folder tree), file table, Preview for images/audio via presigned URL (dropped at `expires_at`), Copy link with expiry shown, Load more, 409 CORS_NOT_READY message. Link from each mount row. ASSUMED shapes: `GET /storage/mounts/{id}/objects?prefix=&delimiter=/&cursor=` -> `{folders: string[] (full prefixes ending /), objects: [{key,size,last_modified|null,content_type?}], next_cursor|null}`; `POST /storage/mounts/{id}/presign` body `{key, method:'GET'}` -> `{url, expires_at}`; 409 code `CORS_NOT_READY`.
- Friend Shelf toggle also in the Friends page row menu ("Share my Media Shelf…" opens it under the row; `showEmpty` explains a missing bucket); test added to `lounge/__tests__/homeFlows.test.tsx`. Role-editor axe test added.
- Friend Shelf toggles (`storage/FriendShelfShare.tsx`, mounted in `lounge/profile/FullProfile.tsx` under the note field, mutual friends only, needs an active user mount). NOT done: a second placement under Settings > Friends (no such section exists; the Friends page is `lounge/shell/FriendsPage.tsx`, its row menu could host the same component). ASSUMED extra route: `GET /users/@me/storage/mounts/{mount_id}/shares` -> `[{friend_id, cinema, library, music}]` (owner's outgoing list). PUT body `{cinema, library, music}` -> the stored share; DELETE when all three are off.
- Role editor Storage group (`settings/server/storage/GrantsCard.tsx`, inside the role's Permissions tab; five switches saved at once via `PUT /guilds/{gid}/storage/grants` body `{ role_id, grants: [...] }` -> 204; current set read from `role.sr_storage_grants`; not in `permissions.ts`, so never in channel overwrites, asserted by a test).
- Connect guidance: least-privilege policy JSON (s3, wasabi, minio; bucket and prefix filled in live from the form) with Copy button and a check-your-provider's-docs line; R2/B2/Garage give scopes in words plus the same reminder (`storage/policy.ts`).
- Guild Settings > Storage (`settings/server/storage/StorageSection.tsx`, section `storage` in the APPS group, visible with MANAGE_GUILD which implies the `manage` grant; the real grant check is server-side): reuses `MountsPanel` with the guild scope.
- BYOS-UI-001 (part): User Settings > Storage (`storage/MountsPanel.tsx`, `StorageSection.tsx`, nav entry `storage`, search index): mount list with status and key hint, connect form with per-provider defaults and guidance, test table (pass/warn/fail), CORS card (check, Apply CORS, manual steps), enable/disable, remove with confirm.

## Still open

Everything in "Resume here".

## Tests run

- Latest: vitest src/storage + user p27storage (22 passed incl. axe), nav modes, friend share; `node --test scripts/check-css-tokens.test.mjs` 6 passed; `node scripts/i18n-audit.mjs` TOTAL 0; tsc and eslint clean.

- `vitest run --maxWorkers=2 src/settings/user/__tests__/p27storage.test.tsx src/i18n`: 55 passed.
- `vitest ... src/settings/server/__tests__/p27storage.test.tsx shell.test.tsx`: 8 passed.
- `tsc --noEmit` (apps/web): clean. eslint on the new files: clean. prettier applied.

## Confirmed by byos-core (applied)

- `key_hint` = last 4 chars of the ACCESS KEY ID (UI shows "Key ID ending xxxx"). ids/owner_id strings, ISO timestamps, endpoint/region nullable. 409 codes MOUNT_CAP_REACHED, MOUNT_LABEL_TAKEN, CORS_NOT_READY get friendly text (`storage/errors.ts`); 403 and 404 too. PATCH status only active|disabled.

## Route shapes assumed (backend must match; types in `storage/types.ts`)

Scope bases: `/users/@me/storage/mounts` and `/guilds/{guild_id}/storage/mounts` (all under `/api/v1`).

- `GET base` -> `Mount[]`; `POST base` (201 -> `Mount`; 409 at the cap).
- `GET/PATCH/DELETE base/{mount_id}`; PATCH accepts `status: 'active'|'disabled'` plus any create field.
- `Mount`: `id, owner_kind (user|guild|platform), owner_id, label, provider (s3|r2|b2|minio|wasabi|garage), endpoint|null, region|null, bucket, prefix, path_style, access_key_id, key_hint, status (active|failing|disabled), cors_ok_at|null, last_test_at|null, last_test|null, created_at`. No secret in any response.
- Create body: `label, provider, endpoint?, region?, bucket, prefix?, path_style, access_key_id, secret_access_key`.
- `POST base/{id}/test` -> `{ at, rows: [{ id, status: pass|warn|fail, message }] }` (same object stored as `last_test`). Probe ids: head_bucket, list, put, multipart, cors, list_buckets. Messages are server sentences (English, not translated by the UI).
- `POST base/{id}/cors/check` and `.../cors/apply` -> `{ state: ok|missing|different|manual, required: string[], found: string[], manual_steps?: string|null, checked_at }`.

Not yet used (planned shapes from the roadmap): shares, grants, shelves summary, objects listing, presign `{ url, expires_at }`.

## Decisions / owner questions

- Provider guidance is plain prose per provider (no IAM JSON block) in `p27-byos-ui-1.ts`; the owner may want exact policy documents later.
- None blocking.

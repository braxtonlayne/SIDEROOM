# Handoff: feat/p27-byos-ui (BYOS-UI-001)

## Resume here

Slice 1 done (commit "BYOS-UI-001: User Settings > Storage"). Next slices, in order:

1. Guild Settings > Storage: add a `storage` section to the server settings (`apps/web/src/settings/server/storage/*`) that renders `<MountsPanel scope={{ kind: 'guild', guildId }} />` (already scope-generic, mocks already serve `/guilds/:gid/storage/mounts`); show it only with the `manage` storage grant or MANAGE_GUILD.
2. Role editor "Storage" group: five toggles (view_cinema, view_library, view_music, upload, manage) in `settings/server/permissions.ts` + `RoleEditor`; payload field `sr_storage_grants`; NOT in channel overwrites (test for that). Route (byos-scopes): `PUT /guilds/{guild_id}/storage/grants`.
3. Friend Shelf toggles (friend profile card, Settings > Friends): `PUT/DELETE /users/@me/storage/mounts/{mount_id}/shares/{user_id}`, `GET /users/@me/storage/shares`.
4. Bucket browser `/storage/{mount}` (route, folder tree, object list, presigned previews, copy link with expiry).
5. Appearance > Sections nav modes (Always show | Always hide | Auto from `GET /users/@me/shelves/summary`) + top tabs hook.
6. `scripts/check-css-tokens.mjs` + test; `e2e/tests/byos-mount.spec.ts` + one `storage.yml` step (do not run Playwright).
7. axe test of the Storage section, `i18n-audit`, bundle budget, `pnpm lint` on whole web (only at the end).

Mocks live in ONE file: `apps/web/src/settings/user/storage/mocks.ts` (`byosMockHandlers`). Tests call `server.use(...byosMockHandlers(opts))` (put per-test overrides BEFORE it; first handler wins). Replace with the real handlers when byos-core/byos-scopes land by deleting that file and its imports.

## Done

- BYOS-UI-001 (part): User Settings > Storage (`storage/MountsPanel.tsx`, `StorageSection.tsx`, nav entry `storage`, search index): mount list with status and key hint, connect form with per-provider defaults and guidance, test table (pass/warn/fail), CORS card (check, Apply CORS, manual steps), enable/disable, remove with confirm.

## Still open

Everything in "Resume here" items 1 to 7.

## Tests run

- `vitest run --maxWorkers=2 src/settings/user/__tests__/p27storage.test.tsx src/i18n`: 55 passed.
- `tsc --noEmit` (apps/web): clean. eslint on the new files: clean. prettier applied.

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

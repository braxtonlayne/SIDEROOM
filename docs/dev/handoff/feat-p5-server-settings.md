# Handoff: feat/p5-server-settings

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p5-server-settings, branch feat/p5-server-settings (base integration/phase4). Local commits only, nothing pushed. Frontend-led, two small backend additions, no migration.

Most of Server Settings already existed (phase 3 and 4); this branch closes the gaps that the earlier handoffs listed and ticks the inventory lines that were built but never ticked. Voice/stage/soundboard, Nitro/boost and Cinema/Library were skipped as asked.

## Done

Frontend (apps/web, `settings/server/`)
- Overview: icon, banner and invite background upload, replace and remove (`ImageField`, data URI to `PATCH /guilds/{id}`, served from the existing media pipeline), live server card preview, AFK channel (voice channels) and timeout wired to `afk_channel_id` / `afk_timeout`, system messages channel kept. A "Not available yet" card lists vanity URL, server tag, traits and widget.
- Roles: role icon (image) and role emoji in the Display tab, replace each other like Discord; icon or emoji shown in the role list; Links tab is a placeholder. List, create, drag and arrow reorder, permission matrix, colour, hoist, mentionable, member assignment, hierarchy locks already existed.
- Members: search and role filter now go to the server (`?query=&role_id=`, 250 ms debounce, cursor paging); member details dialog; the existing timeout, kick, ban, roles and prune dialogs are unchanged.
- Integrations: webhook avatar on create, a new Edit dialog (name, channel, avatar), avatar in the list; Server Template cards get Copy link and Preview.
- Public template page `/template/:code` (`routes/entrance/ServerTemplate.tsx`): preview (channels, roles, usage), name field, Create server (`POST /guilds/templates/{code}`); signed-out visitors are sent to log in and back.
- Audit Log: retention note (45 days default, operator can change it).

Backend
- `MediaKind::WebhookAvatar` (`avatars/<webhook_id>/<hash>`, square, served by the existing public `/cdn/avatars` route) and `MediaKind::RoleIcon` (`icons/<guild_id>/<hash>`, square 256).
- `POST /channels/{id}/webhooks` and `PATCH /webhooks/{id}` (also the token variant) take `avatar` (data URI, `null` removes on modify). A refused request stores nothing; a replaced or cleared image and a deleted webhook delete the object. `SettingsService::new_webhook_id` + `create_webhook(.., id, avatar_key)`; `WebhookPatch.avatar_key`; audit change `avatar_hash`.
- `PATCH /guilds/{id}/roles/{id}` takes `unicode_emoji` (one emoji, `null` clears) and `icon` (data URI, `null` removes). Setting one clears the other and deletes the stored image when the emoji wins. `@everyone` stays immutable for both.
- `docs/05-api-and-gateway.md` updated; `openapi.json` and the api-client regenerated (+28 lines).

## Not done, and why
- Overview: system message toggles (welcome message, sticker reply button, setup tips): the guild has no `system_channel_flags`. Vanity URL: the `vanity_code` column exists (0010) but no endpoint reads or writes it. Server tag, traits, widget: not backed. Banner preset gradient swatches: not built.
- Roles: two-colour gradient (no field), "View Server As Role" (needs the Lounge channel visibility logic with a role override), role icon image is not rendered in the member list or on messages yet (the Lounge only draws the emoji; the hash is on the role object).
- Members: bulk select, sort, join method, "unusual activity" signals, audit history in the details dialog (audit log has no target filter).
- Invites: already complete (create with limits, copy, revoke, pause). Bans: complete (search, reason, unban); no ban-create dialog outside Members.
- Safety setup: verification level, explicit media filter and age-restricted exist; DM and spam protection (LNG-SSET-052), raid auto-lockdown, require 2FA for moderators are not backed.
- AutoMod, Community overview, onboarding and welcome screen already existed and are unchanged; Community setup wizard, Server Guide, Insights and directory requests have no backend.
- Audit retention: the value lives in the admin settings store, which the guild API cannot read, so the note states the default instead of the live number. Exposing it needs a field on the audit-log response.
- Playwright e2e was not run (shared ports 8080/5173).

## Tests
- Web: 635 passing in 54 files (new: overview 2, content 1 webhook avatar, roles 1, people 1, serverTemplate 3, plus updated assertions). tsc, eslint, prettier, `vite build` clean. Run vitest with `--maxWorkers=3`.
- Rust: sr-api lib 314 (+1 `webhook_avatar_is_stored_replaced_and_removed`), `tests/guilds.rs` 14 (+1 `role_emoji_and_icon_replace_each_other_and_clean_up`), sr-chat lib 231 (+1 role emoji/icon rule). Postgres-backed (ignored, throwaway DB, dropped): `settings_repo` 5 passed incl. new `webhook_avatar_key_round_trips_and_clears`. `cargo fmt --check` and `clippy --workspace --all-targets -D warnings` clean.

## Decisions
1. Webhook avatars live under `avatars/<webhook_id>/`, so the public `/cdn/avatars` route serves them with no CDN change. The id is generated before the object is stored (`new_webhook_id`).
2. Role icon and emoji exclude each other server side (Discord behaviour); the UI mirrors it by clearing the other field.
3. The members list stopped filtering client-side: the 20-page cap and the "load everything while searching" trick are gone. Typing a whole user id no longer matches (the server query covers names only).
4. `ImageField` is reusable (hash or data URI value, kind `icons` or `avatars`).
5. Test helpers: the default members handler in `__tests__/helpers.tsx` honours `query` and `role_id`.

## Shared files touched (additive)
`router.tsx` (one route), `settings/server/*`, `routes/entrance/ServerTemplate.tsx` and its test, `docs/05`, `docs/inventory/06`, `packages/api-client` (regenerated), crates `sr-storage` (media kinds), `sr-chat` (role rules, webhook ops), `sr-api` (webhooks, roles, dto, tests).

## How to resume
`npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 install --offline --frozen-lockfile`, `pnpm --filter @sideroom/wasm build`, then in apps/web `tsc --noEmit`, `eslint .`, `vitest run --maxWorkers=3`. Backend with `CARGO_TARGET_DIR=...p4-admin-ui-target CARGO_PROFILE_DEV_DEBUG=0`; Postgres tests need a throwaway DB in `SR_TEST_DATABASE_URL`.

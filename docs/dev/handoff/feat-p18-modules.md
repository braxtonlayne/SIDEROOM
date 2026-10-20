# feat/p18-modules handoff

Branch `feat/p18-modules` (base `integration/phase18` = main). No migration was needed: `feature_enablements` already stores `channel` and `user` scopes (migrations 0003 and 0050). Local commits only.

## Done
- **LNG-CHN-017** (ticked) and **LNG-USET-036** (ticked): channel scope and user scope for feature modules, end to end.
  - **Engine** (`sr-features`): `Field::at(&[Scope])` says where a field belongs (no `at` = platform and server only, so every existing field behaves as before). `SettingsSchema::for_scope`, `validate_at` and `retain_scope`; `set_settings` validates against the level's own fields and `effective_settings` drops keys of other levels. Inheritance already existed (`ScopeRef::chain`): platform, then server, then channel, and platform, then person. New: `FeatureService::overrides` and `clear`, `forget_channel`, `set_enabled` refuses a channel or user flag for a module that does not declare that scope, and `FeatureCtx::channel_config`, `channel_enabled`, `user_config`. The runtime drops a channel's rows on `ChannelDeleted`; account deletion (`sr-db/account_deletion.rs`) drops the person's user-scope rows.
  - **Modules that gained a scope** (conservative, one each):
    - `lounge.message_quotes` gets **channel scope**: switch previews off in one channel, or cap them lower or higher than the server (`max_quotes`). Why: it is the one built-in module whose behaviour is "what happens to messages posted here", it has a single numeric setting, and silencing a busy channel is a very common need (Discord link embeds can be suppressed per channel as well). No other module was touched, so no other behaviour changes.
    - `community.leveling` gets **user scope**: `announce_mine` ("Announce my level-ups", default on). It can only quiet the announcement; XP, level roles and the rank card are unchanged. Why: the existing opt-out deletes a member's XP, and there was no middle road; this is the privacy-friendly one. The setting is read in `sr-server/src/userfeat.rs::award`.
  - **API** (`sr-api/src/routes/settings/features_scopes.rs`): `GET /channels/{id}/sr/features`, `PATCH` and `DELETE /channels/{id}/sr/features/{feature_id}` (`MANAGE_CHANNELS` in the channel, MFA gate on writes, picker ids checked against the server through the shared `check_settings`, which the server-level `PATCH` now uses too); `GET /users/@me/sr/features`, `PATCH` and `DELETE /users/@me/sr/features/{feature_id}` (the signed-in person only; bots refused, also in `auth/policy.rs`). The server and admin views now serve `schema.for_scope(...)`, so a personal field never shows in the server form. The channel view carries `guild_enabled`, `override_enabled`, `enabled` (server AND channel), `settings`, `inherited` (the baseline the form measures overrides against), `overrides`, `schema`.
  - **Web**: Edit Channel > **Modules** tab (`ChannelModules.tsx`, every non-category channel; per-channel switch, SchemaForm, "Follow the server") and User Settings > **Modules** (`user/sections/Modules.tsx`, nav entry, search entry). Both are generic over the schema: a module that declares a scope later shows up without a new screen. The stale "per-channel feature modules are not available yet" note on the Features tab is no longer rendered (its key stays in `srvset-5.ts`, which is not mine to edit).
  - Docs: doc 11 section 1.1 (channel and user scope paragraph, menu names), doc 05 routes row, inventory notes (LNG-CHN-017, LNG-USET-036, FTR-QUOTE-001, FTR-LVL-001).
  - OpenAPI, api-client types and MSW mocks regenerated; route matrix has 6 rows (no `?`); `KNOWN_CLASHES` is still empty.
- i18n fragment `p18-modules-1.ts`.

## Still open
- Nothing from the two items. LNG-CHN-017's `rtc_region` stays closed by decision (one node).
- No other module has a channel or user scope yet. Obvious later candidates: `lounge.auto_thread` (per-channel thread-name template), `chat.reminders` (default snooze), `chat.scheduled_messages`. Each is a small `Field::at` change plus the read in the module.

## Tests run
- `sr-features`: 118 pass (new: scope schema tests, service `clear`/`forget`/narrow-level tests, three `message_quotes` channel tests).
- `sr-api`: whole package (lib 625, `api`, `guilds`, `lounge`, `openapi_names`, `route_matrix`) green; new `features_tests` cover channel list/permissions/narrowing/reset/validation, user list/write/reset/isolation/bots, and that the server form neither shows nor accepts personal fields.
- Postgres (scratch DB `sr_p18_modules`, created and dropped, `--include-ignored --test-threads=1`): `sr-db --test account_deletion` (user-scope rows removed, a friend's kept), `sr-server --lib userfeat_pg` (8, incl. new `a_members_own_switch_quiets_their_level_up_announcement_only`), `sr-userfeat` (33 + 15).
- `cargo fmt --check`, `cargo clippy -D warnings --all-targets` on sr-features, sr-userfeat, sr-db, sr-api, sr-server.
- Web: `tsc --noEmit`, eslint, prettier; new vitest `p18channelModules.test.tsx` (4) and `p18modules.test.tsx` (3); full vitest and build/bundle budget results are in the final report.
- No Playwright, no servers.

## Decisions / owner questions
Conservative defaults taken (Discord has no equivalent module system, so no parity to follow):
1. A channel can only **narrow** a module; it cannot switch on a module the server has off. (Alternative: let a channel opt in on its own. It would break "server owners decide what runs", so not done.)
2. User scope has **no on/off** and is global across servers (not per server). A per-server personal setting would be a different scope.
3. Which modules get a scope: message quotes (channel) and leveling announcements (user), as justified above. Confirm, or name others.
4. `MANAGE_CHANNELS` is checked in the channel (overwrites applied), so a manager who cannot see the channel cannot configure it.

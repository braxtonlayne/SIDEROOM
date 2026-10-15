# Handoff: p11 featmod-a (FTR-CORE/STAR/RR/WEL/JTC/STK/ATH/AP/SLOW/TEMPR/AUTOR/LOG/COUNT/QUOTE)

## Done
- Framework: `sr-features` got `actions` (FeatureActions port), `state` (keyed JSON rows with due time), `runtime` (firehose dispatch to modules enabled in the guild, 30 s tick), `testing` (FakeActions, Harness), `template`. Migration `0680_feature_state.sql`.
- Modules (all default off, per server): lounge.starboard, lounge.reaction_roles, lounge.welcome, lounge.sticky, lounge.auto_thread, lounge.slowmode_plus, lounge.stat_channels, lounge.message_quotes, moderation.temp_roles, moderation.auto_roles, moderation.mod_log.
- AP-001 was already built (AUTO_PUBLISH channel flag).
- Production wiring in `sr-server`: `feature_actions.rs` (system user posts via the message repo, roles/slowmode/rename through the guild backend as the server owner, threads as the message author with owner fallback), `PgFeatureState`, runtime started in app.rs.
- API: `GET/PATCH /guilds/{id}/sr/features[/{feature}]` (MANAGE_GUILD; picker ids must belong to the server; role pickers need assign rights), `GET/POST /guilds/{id}/sr/temp-roles`, `DELETE .../{user}/{role}` (MANAGE_ROLES, hierarchy via real role assignment). Route matrix, OpenAPI and api-client regenerated.
- Web: Server Settings > Features (switch + schema form per module, temp-role panel); SchemaForm now edits lists of groups (rows), enum lists (checkboxes) and uses channel/role choosers. i18n fragment `p11-featmod-1.ts`.
- Generator: `just new-feature <section.name>`, guide `docs/dev/writing-a-feature.md`.

## Open
- FTR-JTC-001: no voice-state events on the bus (join/leave a voice channel) and no move-member action; needs sr-voice hooks first.
- Slowmode presets applied on demand (only schedules exist).
- Reaction roles: no bespoke message picker (message id is typed).
- Actions on the audit log name the server owner, not the module.
- The event dispatcher reads the bus firehose; a lagging bus can drop events (modules are best effort).

## Tests run
cargo test -p sr-features (88), sr-api features_tests + route_matrix, sr-server Postgres `feature_actions_pg` (6, --test-threads=1), clippy -D warnings and fmt on sr-features/sr-api/sr-server, web: tsc, eslint, prettier, vitest settings/server + admin + i18n + router, build + bundle budget.

## Decisions / owner questions
- Starboard post is removed when the count falls below the threshold; a moderator-deleted post is not reposted.
- Module actions run as the owner (audit attribution). Acceptable, or add a system actor to the audit log?
- Reaction-role "verify" mode = grant once, un-reacting keeps the role.

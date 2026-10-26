# Handoff: feat/p26-expressions (PAR-FU-003)

Branch `feat/p26-expressions` (worktree `p26-expressions`), base `integration/phase26`. Nothing pushed or merged. Migration 1360 only (1361 to 1369 untouched). No new dependency, no download. Scratch database `sr_p26_expressions` was created and dropped.

## Resume here

Everything in parts (a) to (f) is built and tested (see "Tests run"). What is left is outside this agent's limits:

1. Run the real load test (`load-test.yml`, or `sr-loadtest` against a release `sr-server`) to confirm the message-create budget of RDY-PERF-002 with the gate in; this agent could not start servers or ports. The figures below are a Postgres micro-measurement of the same code path.
2. After the merge: the `CHANGELOG.md` lines below go into the integrator's Phase 26 section (see "CHANGELOG lines for the integrator").
3. The owner questions at the end.

## What was built

* **(a) The bit.** `Permissions::USE_EXPRESSIONS_EXTERNALLY = 1 << 60` (`sr-core`); in `default_everyone_permissions`, not in `dm_permissions` (`sr-chat/src/perms.rs`); migration `1360_use_expressions_externally.sql` (the shape of 0791); doc 07 section 2 has the row and an "External expressions (D-375, D-376)" paragraph; `apps/web/src/settings/server/permissions.ts` has `p('USE_EXPRESSIONS_EXTERNALLY', 60, 'sideroom', { scope: 'guild' })` with its catalog name and description (fragment `p26-expressions-1.ts`), so the role editor shows it in the SIDE ROOM group and `overwriteApplies` keeps it out of channel overwrites. `sr-permissions-wasm` needed no code; its test now covers bit 60 in `ALL` and the admin bypass.
* **(b) The source gate.** `sr_chat::perms::may_use_externally(guild, member, roles)` (guild permissions only, owner and administrator bypass from `compute_base`), plus `source_standing(repo, source, user)` / `SourceStanding::permits(standing, available, role_ids)` which load the caller's membership and roles in the source server and apply the expression's own `available` and `role_ids`. A user who is not a member (never joined, left, unknown server) has no standing and fails closed.
* **(c) Enforcement.**
  * Messages: `rich_create::gate_external_emoji` / `refused_emoji` run in `create_message` right after the channel is resolved (before AutoMod, hooks and storage) and in `edit_message` (only when the new content has custom emoji). A refused `<:name:id>` / `<a:name:id>` becomes `:name:` (`rich_model::rewrite_emoji_tokens`); the message goes through. Own-server emoji, application emoji (D-237) and ids that match no server emoji are untouched.
  * Stickers: `resolve_stickers` accepts another server's sticker only with `USE_EXTERNAL_STICKERS` in the target (a DM always) and the source gate; otherwise `400 UNKNOWN_STICKER` as before. In a DM another server's sticker can now be sent.
  * Reactions: `add_own` answers `403 50013` for a custom emoji of another server that fails either gate (checked even when the emoji is already on the message, because both gates always apply, D-376 (b)).
  * Sounds: `send_sound` adds `may_use_externally` beside the `USE_EXTERNAL_SOUNDS` check, `403 50013`; a non-member already failed in `guild_access`.
  * New `RichStore::guild_emojis(&[u64]) -> HashMap<u64, GuildEmojiRef>` (Postgres: one query joining `emojis` and `emoji_roles`; memory: a test seed map).
* **(d) The listing route.** `GET /users/@me/expressions` (`settings/expressions.rs`, session and bot token, not OAuth bearer): `[{guild:{id,name,icon}, usable_externally, emojis, stickers, sounds}]`, lists filtered by `available` and emoji `roles`. Route matrix row (`GET /users/@me/expressions authenticated self allow - -`), OpenAPI and the TypeScript client regenerated, mocks regenerated (the web test server also answers `[]` by default).
* **(e) The pickers.** `lounge/components/myExpressions.ts`: one cached fetch per session, dropped on `GUILD_EMOJIS_UPDATE`, `GUILD_STICKERS_UPDATE`, `GUILD_SOUNDBOARD_SOUNDS_UPDATE`, `SOUNDBOARD_SOUNDS`, `GUILD_ROLE_CREATE/UPDATE/DELETE`, `GUILD_CREATE`, `GUILD_DELETE` and the caller's own `GUILD_MEMBER_UPDATE` / `GUILD_MEMBER_REMOVE`. `EmojiPicker`, `StickerPicker` (in `ExpressionPicker.tsx`) and the soundboard popover get one section per other server in sidebar order (the current server stays first, its own section); servers whose `usable_externally` is false are omitted; when the target channel lacks the bit (`ChannelPermissions.externalEmoji / externalStickers / externalSounds`, from the permission WASM) the sections are shown greyed with a tooltip (`title`), tiles `aria-disabled`, and picking does nothing. The category bar gets the server's icon (or initial). In a DM the emoji and sticker pickers now offer other servers' expressions (before, custom emoji were offered only inside a server). Composer/ChannelView got two props to carry the channel bits.
* **(f) Docs.** Doc 07 section 2, doc 05 section 5.3 ("External expressions", the route and the three refusal shapes), this handoff. `docs/13-roadmap-and-phases.md` does not list expression parity (nothing to change).

## Message-create cost (RDY-PERF-002 guard)

The gate is one batched `guild_emojis` query, made only when the content contains a `<:name:id>` token; a message without custom emoji pays one string scan. Measured over a real Postgres (scratch database, debug build, shared laptop, median of 200 runs, `cargo test -p sr-api --lib external_pg -- --include-ignored --test-threads=1 --nocapture`):

| Step | Before (no gate) | After |
|---|---|---|
| `access` + `send_in` of a plain message | 1.211 ms | 1.211 ms + gate |
| gate, message without custom emoji | none | 0.0023 ms |
| gate, one own-server emoji (1 query, nothing else) | none | 0.253 ms |
| gate, one foreign emoji (1 lookup + membership, server and roles, once per distinct source server) | none | 0.643 ms |

So a plain message costs +0.2 percent, a message with an own-server emoji about +21 percent of the send path and a foreign emoji about +53 percent. The full `sr-loadtest` figure (2,000 sessions, 200 msg/s) was NOT run: it needs a release server and ports, which the wave's limits forbid. The seeded load dataset posts plain text, so it exercises the plain-message line. Note the roadmap's "skipped when every id belongs to the channel's own server" cannot be decided without the lookup (emoji ids carry no server); the lookup is skipped for messages without tokens, and a cache of id to server (immutable) is the next step if the owner-dispatched run shows a cost.

## Tests run

All with `-j 2`, one cargo command at a time, scratch database `sr_p26_expressions` (dropped at the end).

* `cargo fmt --check` and `cargo clippy -D warnings --all-targets` on sr-core, sr-chat, sr-db, sr-api, sr-permissions-wasm: clean.
* `cargo test` sr-core, sr-chat, sr-api, sr-permissions-wasm: all green (sr-api lib 685 passed, 16 ignored; includes `route_matrix` 6/6 and `openapi_names` 2/2 with the new row and schemas; `KNOWN_CLASHES` stays empty). New: sr-core bit test, sr-chat `may_use_externally` and `SourceStanding::permits` (owner bypass, admin bypass, role without the bit, non-member), `rewrite_emoji_tokens`, and 13 endpoint tests in `settings/external_expressions_tests.rs` (message kept when both gates pass; rewritten when the target lacks bit 18; rewritten when the source role lacks bit 60; rewritten when the sender left the source; kept in a DM when only the source gate passes; own-server, application and unknown-id emoji untouched; unavailable and role-restricted emoji; edit; stickers both outcomes; reactions 403 for each gate; sounds 403 for each gate; the listing with one entry per server and the right `usable_externally`; a member of no server gets `[]`; no credentials 401).
* Postgres (`--include-ignored --test-threads=1`): `sr-db --test p26_expressions` 2/2 (new servers carry bit 60; migration 1360 backfills an old `@everyone` role, leaves other roles alone, idempotent; `guild_emojis` batch with role restrictions, unavailable emoji, application emoji and unknown ids) and `sr-api --lib external_pg` 1/1 (the message rewrite over real Postgres plus the cost figures above).
* Authorization matrix: `cargo test -p sr-server --test authz_matrix -- --include-ignored --test-threads=1` with `SR_AUTHZ_GROUPS=5` (the group that holds the route): `rest: proven (8133 calls, 0 failures, 16 weak refusals, 5 masked)`, and the report shows the new route answering 401 to anon, 403 to both OAuth bearers, `[]` to outsider and bot_out, and the member's servers to the others. The other groups do not contain the row and were not rerun. (A first run with `SR_AUTHZ_WRITES=0` reported the known weak-refusal ceiling of 16 exceeded, 38 weak; that ceiling only holds with the write phase on.)
* Web: `tsc --noEmit` clean; eslint clean on every touched file; prettier clean; full `vitest run --maxWorkers=2` 304 files / 2,642 tests green (after `pnpm --filter @sideroom/wasm build`); new `externalExpressions.test.tsx` (9 tests: sections per server, omitted servers, greyed state with the tooltip, DM, no custom handler, fetch once and refetch on a gateway event, axe, stickers greyed, stickers in a DM, bit 60 in the SIDE ROOM group and out of overwrites) plus the existing `emojiPicker.test.tsx` axe check stay green; `moderation.test.tsx` covers bit 60; `vite build` and `scripts/check-bundle-budget.mjs` pass (lounge 433.8 KB of 600 KB gzip). `i18n-audit.mjs` still reports the 41 strings of the i18n agent's 14 files and none of mine.
* `@sideroom/api-client`: `mocks:check` up to date (973 operations), `typecheck` clean.

## Decisions

* Both gates always (D-376 (b)); refused emoji become text, refused stickers 400, refused reactions and sounds 403.
* An emoji's own role restriction has no owner or administrator bypass in the external check (conservative: the restriction is the source server's choice).
* A reaction with another server's emoji is gated even when the emoji already sits on the message (Discord skips the external-emoji check there; D-376 says both gates always).
* The three picker tooltips are three catalog keys (emoji, stickers, sounds), not one, because each names what it greys out.
* Gateway invalidation drops the cached list on any role or expression change in any server; it does not try to work out whether the caller was affected.

## Not done

* Webhook execution, interaction responses and thread starter messages write message content without going through `create_message`; they do not rewrite refused emoji (the roadmap names create and edit only). Their authors are bots or webhooks acting for a server; flagged for the owner below.
* No Playwright spec for the picker (Vitest covers sections, greyed state, tooltip, axe).
* The `CHANGELOG.md` section is left to the integrator (four agents would each add a Phase 26 heading and conflict).

## CHANGELOG lines for the integrator

- **Emoji, stickers and sounds work everywhere.** Your server's emoji, stickers and sounds can be used in your other servers, in DMs and in group DMs. The emoji, sticker and soundboard pickers show one section per server you belong to.
- **Servers decide whether their expressions leave.** A new role permission, "Use Expressions Externally" (on by default), controls whether members with a role may use the server's emoji, stickers and sounds elsewhere. A channel that does not allow external emoji, stickers or sounds shows other servers' sections greyed out with the reason.

## Open owner questions

1. Should webhooks, interaction responses and thread starter messages also rewrite refused emoji (today they pass through untouched)?
2. Should owners and administrators of a source server bypass an emoji's own role restriction when using it elsewhere (today: no)?
3. Is the message-create cost of a foreign emoji (about 0.6 ms of added database work) acceptable, or should an emoji-to-server cache be added after the owner-dispatched load run?

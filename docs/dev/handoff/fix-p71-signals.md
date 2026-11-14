# Wave 71 signals handoff (SEC-AUD-257..261; D-732, D-733, D-735, D-744)

Branch `fix/p71-signals` (base `integration/phase71` at `0c6b26d8`). Items 1 to 5 done. No migration (item 5 needed none), no route or response shape change: nothing for the commander to regenerate. No new i18n key. Nothing pushed. Nothing of mine is running; both scratch databases are dropped and `p71-signals-target` and `p71-signals-bin` are deleted.

## Done (commits)
1. `dca0a794` SEC-AUD-257: `hub.rs` (typing audience), `sr-gateway/src/tests.rs`, and `routes/messages/typing.rs` (DM, see below). The DM typing test sits in `routes/social/tests.rs`, committed with item 4.
2. `7b37d62d` SEC-AUD-258: `message_hooks.rs`, `social/{service,notifications,repo,memory,tests}.rs`, `sr-db/src/social_repo.rs` + `sr-db/tests/social_repo.rs`, `sr-api/.../dynrepo.rs` (signature only).
3. `a36e13db` SEC-AUD-260: `preview.rs`, `preview_cases.json`, `lounge/msg/excerpt.ts`, `p70Plain.test.tsx`, the route tests in `routes/messages/tests.rs` and `routes/social/tests.rs`.
4. `a542660a` SEC-AUD-259: new `lounge/shell/notifDefaults.ts`, `desktopNotify.ts`, `useShellBoot.ts`, `settings/user/sections/Notifications.tsx`, `lounge/__tests__/p71Signals.test.tsx` (holds tests for 258, 259 and 260).
5. `1c778803` docs and the shots spec; a last commit with the shots-spec fix and this handoff.

Outside my file list, flagged: `crates/sr-api/src/routes/messages/typing.rs` (one guard, item 1's DM path, the task asked for it), `crates/sr-api/src/routes/social/dynrepo.rs` (a signature follows the trait), `routes/social/tests.rs` (the DM typing test).

## Item 1, SEC-AUD-257: typing across a block
- Confirm: `typing_does_not_cross_a_block` (gateway): 1 and 2 block each other, 4 is a third member; 1 types, 2 hears nothing, 4 and 1's own session do; 2 types, 1 hears nothing. Also `typing_is_unchanged_when_nobody_is_blocked` (no audience set, a block between two others changes nothing). **I did not run these on the base** (a second cold build was not worth it): the base set `audience: None` for `TYPING_START`, so B's session received it by construction.
- Fix: `Hub::on_guild_event`, for `TypingStarted { user }`: `dir.blocked_peers(user)`; non-empty gives `audience = guild_users(guild) minus them`.
- **Cost of one typing event:** one directory call, not cached: `gateway_pg.rs:705`, one `UNION` of two indexed lookups on `relationships` (PK and `relationships_target_idx`). The web sends at most one typing event per channel per 8 s (`Composer.tsx:839`); any other client is held to the route's request limit.
- **DM path:** typing in a DM is published by `typing.rs` to each recipient's user topic (`deps.publish`, `dm_channel()`), so the hub's guild path never sees it. It could cross a block in a 1:1 DM: the route never called `require_dm_open` (sends, reactions and polls do). Now a closed 1:1 DM answers 204 and publishes nothing (silent, no 50007: typing then tells the typist nothing a send would not). Group DMs are never closed by a block. Test: `a_block_closes_typing_in_a_one_to_one_dm` (before the block Bob's topic gets `TypingStarted`; after it neither side gets one). Not run on the base.
- **Bridge:** `bridge_inbound.rs:316`: `user` is `uid(discord_user_id)`, a Discord member's id, which is in no `relationships` row, so the rule never applies there (one extra empty query per bridged typing event). File unchanged.
- The web's `useLiveTypingIds` filter stays (ignored accounts, older server).

## Item 2, SEC-AUD-258: the badge hook knows roles
- Confirmed: a message with `@everyone` and a role mention; a member who suppresses `@everyone` only and does not hold the role got a count of 1 (the old call passed `roles = !role_ids.is_empty()`). Test `a_role_the_member_does_not_hold_does_not_reach_them` (memory) and the Postgres twin (`suppress_switches_keep_the_badge_off`, now with a real server, two roles and `member_roles` rows). Not run on the base.
- `suppressing_scopes(guild, users, role_ids)` returns `(user, suppress_everyone, suppress_roles, holds_role)` from one statement (`EXISTS` on `member_roles`); the default method of the trait is gone (it could not read roles), memory and Postgres implement it. `badge_suppressed(everyone, roles, se, sr)` is unchanged: `roles` now means "holds a mentioned role".
- **What I saw about `notify`:** `fanout_recipients` (`service.rs:409`) puts a role-only mention's recipients in `notify` only if they hold the role, so a non-holder is in `notify` only through `@everyone`: they were the over-count case and nobody else.
- Open app's gate (`mentionRules.ts`): it already takes `kind.role` from the viewer's own roles. The vitest in `p71Signals.test.tsx` **passed on the base**; file not changed.
- Still no lookup for direct-mention-only messages (`no_scope_lookup_for_a_message_with_direct_mentions_only` green). The existing `role_only_follows...` test needed `hold_role` calls (the rule changed what its recipients are).

## Item 3, SEC-AUD-259: the person's own default
- `notifDefaults.ts` (store `level`, `load`, `set`, `startNotifDefaultsLive`), started from `useShellBoot`, read by `useDesktopNotify` as `userDefault`; Settings writes through `setNotifDefault`. A read in flight when a newer event arrives does not overwrite it (a version counter). A failed read leaves `undefined`, tried once per boot.
- Confirm test: person default 2, server default 0, nothing chosen: a message raised nothing wanted; on the base the vitest run had **10 of 15 failing** (confirm, update event, Settings write-through, the seven closer cases); with the change all 15 pass.
- **First-paint grep:** `notifDefaults` is imported by `desktopNotify.ts`, `useShellBoot.ts` (both imported only by `LoungeShell.tsx` and `DmToasts.tsx`, Lounge chunk) and `settings/user/sections/Notifications.tsx` (Settings chunk). None of `main.tsx`, `router.tsx`, `*/routes.tsx`, `pwa/**`, `i18n/**`, `gateway/**`, `store/**` imports it. I did not build the web bundle (nothing public or first-paint changed).
- **The two recorded notifications (real stack, `p71-signals-shots.spec.ts`, faked `Notification`):** the Owner has the Lounge open on Home; in a second tab Settings > Notifications sets "Nothing"; Bob writes in a server with nothing chosen: **0 recorded** (event path across tabs, no reload). The Owner sets "All messages": Bob writes: **1 recorded** (`body: "after all: heard again"`). Baseline before: 1 recorded after 2 messages (a fresh page's gateway needs a moment; the spec writes until the first is heard).

## Item 4, SEC-AUD-260: a closed cut
- Confirmed on the base for the web (vitest: all seven kinds failed with `null`, see item 3's count). The Rust twin `a_long_run_of_markup_is_closed_at_the_cut` and the two route tests were **not run on the base**; the old route tests asserted `look…` which the base satisfied and mine no longer does.
- Rule as the contract: first cut only, closers `||`, `**`, `__`, `~~`, `*`, `_`, backtick; first whose plain text (trimmed, non-empty) is a prefix of the whole's wins; result is cut text + closer + ellipsis; else D-728's move-back unchanged. Spaces before the closer cannot occur (the cut is trimmed first). Both sides, `excerptPlain` untouched.
- The bold case did give nothing on the base (parser reads an unclosed `**` as text, so the prefix test failed and the cut moved back to nothing).
- **Changed `want`s of `preview_cases.json`** (nothing else changed; the cases where no closer fits, mention, emoji, link, short, whole, stay):

| case | before | after |
|---|---|---|
| a spoiler split at the cut | `a…` | `a ||` + 136 x + `||…` |
| bold split at the cut | 130 x + `…` | 130 x + ` **bold wo**…` |
| a code span split at the cut | 130 x + `…` | 130 x + ` `` `some cod` ``…` (backtick-closed) |
| one long spoiler (renamed "reads as a closed spoiler") | no text | `||` + 138 x + `||…` |

New cases: one long bold, italic, underlined, struck, code span (each `first words…closer…`), and a spoiler inside bold (no single closer fits: `null`, falls back to the move-back, as D-728 says).
- D-728 kept: the hidden words are inside closed bars, read as plain text `[spoiler]`. Wave 70's `a_cut_never_leaves_a_spoiler_open` and the web "confirm (b)" were updated to assert that (the Rust one asserted the letters were absent from the raw text; now they are in the text but never in the plain text); `a_spoiler_inside_bold_never_leaks_through_a_closer` is new. The two wave 70 route tests now assert `look [spoiler]` as plain text instead of `look…`.
- **Worst case:** `worst_case_time_is_small` (debug build, 4,400 characters of nested markup): **1.5 ms** (wave 70: 0.88 ms; run noise included).
- **Shots:** `visual-sweep-shots/p71-signals`, **36 PNGs, all opened by me** (thread chip spoiler/bold, loaded/live; request rows spoiler/bold, loaded/live; desktop 1280x800 and phone 375x812; Editorial, Lounge Light, Lounge Dark). The spoiler chip reads `look [spoiler]…`, the bold one the sentence's words; the request rows the same (the sender shows "Filed as likely spam" and the line after "Show message"). Seen in passing, not mine: the live DM toast already reads `look [spoiler]`; on a phone a stray small "W" sits under a long author name in message headers (pre-existing layout, not looked into).

## Item 5, SEC-AUD-261: the Inbox mention query on a filled database
Seed (scratch script, not in the repo): 300,000 messages over 10 channels and 30 days (70,274 in the last 7 days, the query's window), 2,000 mentioning caller 1 and 2,000 caller 2, 5,002 `relationships` rows, 5,000 `user_ignores`; caller 2 has 250 block rows (200 blocked, 50 blocking) and 100 ignores; `ANALYZE`.
- Plan: per-partition `Index Scan Backward` on the `messages_YYYY_MM_pkey` over the id range (the 7-day window; the future partitions are empty: 2 buffers each), a hashed subplan for the caller's channels, `Merge Append`, then the `NOT EXISTS` probes per surviving row.
- Time (`EXPLAIN ANALYZE`, first and second run): caller 1 (no blocks): **8.8 ms, 3.3 ms**; caller 2 (250 blocks, 100 ignores): **9.4 ms, 4.6 ms**. Planning 62-66 ms cold, 2.4 ms warm (the 12 future partitions).
- Indexes: blocks use `relationships_pkey` through a BitmapOr of two probes (both directions, `(user_id, target_id)` and `(user_id, target_id)` reversed: no step uses `relationships_target_idx` here, the PK serves both because the pair is fully bound); ignores use an Index Only Scan on `user_ignores_pkey`. No step scans a whole partition: the current month's partition is read over the 7-day id range only (8,474 buffers, stops at the limit).
- Under 50 ms and nothing scans a partition: **no migration** (3060 not written).

## Tests run
- Rust: `cargo test -p sr-gateway -p sr-chat -p sr-api -p sr-db` by name (typing, suppress_badge, preview, the four route tests, the shared notification cases): all green after one fix (the wave 70 spoiler test); `suppress_switches_keep_the_badge_off --ignored`: green. `cargo clippy -p sr-gateway -p sr-chat -p sr-db -p sr-api --all-targets -- -D warnings`: clean. `cargo fmt` on the four crates touched only my files. I did not build or clippy `sr-server` separately beyond the binary build for the stack.
- Web: `p71Signals.test.tsx` (15), `p70Plain` (64), `desktopNotify.test.ts` (57), `p70Mentions`, `p67Previews`, `p68Previews`, `p69Threads`, `p69Notify`: all green. tsc, eslint, prettier on my files clean; `i18n-audit --max 0`: 0.
- e2e: `p71-signals-shots.spec.ts` (local, `SR_P71_SIGNALS=1`): both tests pass on a stack built from this branch.

## Docs updated
`docs/05-api-and-gateway.md` (new "Typing and blocks (D-744)" paragraph before the age gate; "What counts as a mention": the role sentence; the request preview and thread row paragraphs for the closed cut), `docs/specs/lounge-chat.md` (the typing bullet, section 10's order and the person's default, the request and thread-row preview sentences). `check-doc-links` not re-run after the last edit (links unchanged). Nothing outside these needs a sentence from me except the inventory and changelog below.

## Proposed inventory rows
- SEC-AUD-257: `TYPING_START` crossed a block (the gateway sent it to every channel viewer; only presence left blockers out); also typing in a blocked 1:1 DM. Fixed: audience without blocked peers; the typing route is closed by `require_dm_open`. Tests `typing_does_not_cross_a_block`, `a_block_closes_typing_in_a_one_to_one_dm`.
- SEC-AUD-258: a member who suppressed `@everyone` only got a badge from a message with `@everyone` and a role they did not hold (D-726 treated any role mention as reaching them). Fixed: `suppressing_scopes` returns `holds_role`. Closes the over-count of SEC-AUD-241.
- SEC-AUD-259: the open app skipped the person's default level. Fixed (D-732). Closes SEC-AUD-240's "Open:" clause.
- SEC-AUD-260: a preview of one long spoiler or bold sentence was empty. Fixed (D-735), D-728 kept.
- SEC-AUD-261: the Inbox mention query measured on 300,000 messages: 3-9 ms, no index needed.

## Proposed threat-model rows
- T78: typing as activity that crossed a block. A blocked account's `TYPING_START` told the other person when and where the blocker was writing. Closed in the gateway (both directions) and in 1:1 DMs; residual: group DMs (never closed by a block, as on Discord), ignored accounts (web-side filter only).
- T79: the closed cut as a place a spoiler's words travel. After D-735 a preview's `content` holds the first characters of a spoiler inside closed `||` bars (`look ||the swordfi…||…`), so the words are on the wire inside bars. Residual: a client must hide them as it hides any spoiler (the Lounge draws `[spoiler]`); a client that prints the raw `content` shows them. Nothing the server never sent before is sent except those words; the whole-message prefix test still forbids an unclosed spoiler.

## CHANGELOG sentences (ops's file)
- Members: a blocked person's typing never reaches you, and typing in a direct message stops with a block; "Suppress @everyone" now keeps the badge off for a message that also pings a role you do not hold; the desktop notification follows your own default level; a thread or message request whose latest message is one long spoiler or bold sentence shows a preview again.

## Dead keys
None. No string added or removed.

## Not done
Nothing dropped. The web bundle and entrance checks were not run (nothing public or first-paint touched).

## Owner questions (for the Fable agent)
1. A typing event in a blocked 1:1 DM now answers 204 and is dropped silently. Discord answers an error (50007) like a send. Silent chosen so typing tells the typist nothing; say if the error is wanted.
2. A closed spoiler in a preview carries the first ~130 characters of the hidden text on the wire, inside bars (T79). Acceptable for the web's own rendering; if third-party clients that print raw `content` matter, the server could send the closed spoiler with the words replaced by a placeholder run: a new field or a different rule from D-735.
3. "Set it back" in the notification check used "All messages" (the UI has no "follow each server" choice); `null` is only reachable by the API.

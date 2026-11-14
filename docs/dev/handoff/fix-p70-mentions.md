# Wave 70 mentions handoff (SEC-AUD-237..241; D-721, D-725, D-726, D-727)

Branch `fix/p70-mentions`, base `integration/phase70` (`966e5a42`). All five items are done. No migration, no route or response shape change: `openapi.json` and the generated client are untouched (nothing for the commander to regenerate).

## Done (commits, oldest first)
- `SEC-AUD-237` the open app's mention count follows the server (D-721): `store/mentionGate.ts` (new), the `MESSAGE_CREATE` lines of `store/reducer.ts`, `lounge/shell/mentionRules.ts` (the Lounge's gate, with the suppress rules of 241), the registration in `useShellBoot.ts`; docs `docs/05` (one new paragraph, "What counts as a mention") and `docs/specs/lounge-chat.md` section 10.
- `SEC-AUD-238` the Inbox's mention list leaves out blocked and ignored authors: `inbox_pg.rs` (`MENTIONS_SQL` gets two `NOT EXISTS`).
- `SEC-AUD-239` a reply pings unless turned off (D-725): `service.rs` (`send_resolved`), `push_notify::mention_kind`.
- `SEC-AUD-241` the suppress switches keep the badge off (D-726): `message_hooks.rs`, `SocialService::mention_suppressed`, `notifications::badge_suppressed`, trait method `NotificationRepository::suppressing_scopes` (default impl; Postgres one-query override; `MemoryRepo` override that counts lookups; `DynRepo` delegates). This commit also holds `social/notify_cases.json` and the Rust test that runs it (240), because both live in `social/tests.rs`.
- `SEC-AUD-241` (second) the Postgres test and the forward test fix.
- `SEC-AUD-240` the desktop notification decides as the engine does (D-727): `desktopNotify.ts`, its tests, `e2e/tests/p70-mentions-shots.spec.ts`.
- A last commit: rustfmt of my files, final spec edits and this handoff.

## Not done / limits
- The person's own default level (`/users/@me/notification-defaults`) is NOT in the client without a request: it has no store, and `SR_NOTIFICATION_DEFAULTS_UPDATE` is not handled anywhere in the web. `NotifyContext.userDefault` exists and is tested (shared cases) but `useDesktopNotify` does not fill it, as the task says. The server's default (`default_message_notifications`) is on the guild object the store already holds (typed only in `settings/server/api.ts`; read by a cast in `desktopNotify.ts`).
- Item 241, both kinds on one message: the hook does not know a recipient's roles, so with `@everyone` plus a role, a member who suppresses only `@everyone` and does not hold the role still gets the badge (over-count, never an under-count). A member who suppresses both gets none.
- The live count of an author who blocked the viewer still counts until a reload (the client is never told who blocked it): documented in docs/05 and lounge-chat.md.
- The Inbox's own two switches were left as they are (the task says so).

## For each item: confirm test, and whether I watched it fail on the base
- **237** web: `store/mentionGate.test.ts` and `lounge/__tests__/p70Mentions.test.tsx`. I reverted `reducer.ts`, `desktopNotify.ts` and `useShellBoot.ts` and ran them: 4 of 6 `p70Mentions` tests failed (gate, role, role notification, muted category), and the `mentionGate` reducer tests need the new file so they cannot run on the base. Server half (c): the SEC-AUD-011 test existed for the block case (`a_blocked_users_mention_does_not_badge_the_blocker`); I added `an_ignored_or_blocking_authors_mention_adds_no_count` (ignore, and the author-blocked-target direction). It passes; it is a guard, not a failing test (the server already did this). **What a reload shows against the open app:** before, the open app counted a blocked or ignored author's mention (badge, then gone after a reload) and never counted a role mention (missing, then there after a reload). After: both agree. Shot proof: step e (live) and step f (after reload) show the same label and the same server count (0).
- **238** `inbox_pg::tests::leaves_out_mentions_by_blocked_and_ignored_authors` (Postgres, passes): blocked by the caller, blocked by the author, ignored by the caller are out; an unrelated author's is in; a limit of 2 fills from the older rows when three newer ones are filtered. I did not run it against the base SQL (I wrote the filter first), so I did not watch it fail.
- **239** `a_reply_pings_the_replied_to_author_unless_turned_off` (sr-api route), `service_tests::{a_reply_without_allowed_mentions_pings_the_replied_to_author, a_reply_with_the_ping_off_mentions_nobody_and_with_it_on_as_before, a_reply_to_ones_own_message_or_to_someone_who_cannot_see_the_channel_adds_nobody, a_forward_is_not_a_reply}`, `push_notify::tests::mention_kind_reads_users_roles_and_replies` (changed: a reply that did not ping is `reply: false`). All pass. I did not run the base. Note: a forward cannot carry `reply_to` (`FORWARD_SHAPE`), so the forward test only proves a forward mentions nobody; the `forward.is_none()` guard is belt and braces.
- **240** vitest: 36 shared cases plus 7 targeted cases in `desktopNotify.test.ts`; on the base 19 of the 57 failed (role, category mute, category level, server default, person default, reaction in a muted category). The Rust twin `the_shared_notification_cases_hold` passes on the existing engine (it only reads the file). `desktopNotify.test.ts` old cases still pass with the old context literals.
- **241** `social::tests::suppress_badge::*` (memory repo, includes "no lookup for direct-only", counted by `suppress_lookups`) and the Postgres `suppress_switches_keep_the_badge_off` (passes). Not run on the base.

## Senders of `SendMessage` and what they pass for replies
Only the message endpoint (`routes/messages/send.rs`) sets `reply_to` (and `allowed_mentions` from the body). Others never reply (`reply_to: None`), so the new default does not touch them: developer interactions (`..default`, `allowed_mentions: None`), webhooks (`None`), forum post starter (`None`), the Staff feed (`Some(AllowedMentions::default())`, nothing allowed), the user-feature cards (`Some` with only the card's ping lists), the search-job test. The Discord bridge's inbound posts go through its own sink (`NewPost`, with `reply_to`), not `SendMessage`, so a bridged reply is unchanged. No system sender needed changing.

## Where the two notification defaults live in the client
Server default: `default_message_notifications` on the guild object in the store (`st.guilds[id]`, untyped there). Person's default: `GET/PATCH /users/@me/notification-defaults` (`settings/user/api.ts`, shown in Settings > Notifications); not cached, so left out. For a thread, `categoryId` is `channels[id].parent_id` exactly as `push_notify.rs` passes `ch.parent_id`: a thread's parent is its parent channel, so that channel's mute and level count as the "category" step, and a thread's grandparent category does not.

## Query plans (item 2)
Scratch DB `sr_p70_mentions` (migrated, empty tables, so costs are small and a planner on empty tables is not proof). The two `NOT EXISTS` are keyed by the tables' primary keys: `relationships_pkey (user_id, target_id)` (and `relationships_target_idx (target_id, type)` for the reverse direction) and `user_ignores_pkey (user_id, target_id)`. With `enable_seqscan=off` the plan was: per-partition index scans on `messages_*_channel_id_id_idx1` (id range), then a Hash Anti Join against `user_ignores` and a Nested Loop Anti Join against `relationships`. On an empty database the planner hashes the two small tables rather than using their keys, which is the right choice for tiny tables; for a real database with the seven-day bound and `LIMIT $6` I did not measure. Still one statement.

## First paint
`app-*.js` gzip: before 215,594 bytes (wave 69 figure), after 215,739 bytes (room 40,261). +145 bytes for the reducer lines and `mentionGate.ts` (budget 600). `check-bundle-budget`, `check-entrance-isolation` and `check-staff-delivery` all passed after the build.

## Tests run
- clippy `-D warnings` on sr-chat, sr-db, sr-server, sr-api (all targets): clean. `rustfmt` run on my Rust files.
- Rust: sr-chat (`suppress_badge`, `the_shared_notification_cases_hold`, the four reply tests), sr-server (`mention_kind_reads_users_roles_and_replies`, both inbox Postgres tests), sr-api (3 tests), sr-db (`suppress_switches_keep_the_badge_off`): all pass.
- Web: tsc, eslint and prettier on my files, `i18n-audit --max 0` (TOTAL 0), vitest `mentionGate.test.ts`, `desktopNotify.test.ts`, `p70Mentions.test.tsx`: pass.

## Shots
`C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p70-mentions`: 36 PNGs (6 steps x desktop 1280x800 and phone 375x812 x Editorial, Lounge Light, Lounge Dark), from `e2e/tests/p70-mentions-shots.spec.ts` (local only, `SR_P70_MENTIONS=1`), 6 tests, all pass. **I opened 11 of the 36** (at least one of each step a to f, desktop and phone, in all three themes), not all 36; the others come from the same code path with only the theme changed, and every shot is asserted by the spec (labels, server counts, notification records). Read: badge `1` on `talk` and on the server after a direct mention, `2` after the role mention, `3` after a pinging reply, no new badge for the quiet reply; in `talk` the pinging reply row is highlighted and the quiet one is not; after blocking Bob his mention leaves `talk` unread (dot) with no badge, live and after a reload alike. The server rail shows leftover servers of earlier runs of the same scratch DB (not a defect). The phone shots of steps a, b, c, e, f show the channel list (the badges live there); step d is the message list.
- Desktop notifications (faked `Notification`): per run, the direct mention, the role mention ("... mentioned you") and the pinging reply each recorded one; the quiet reply and the blocked author's mention recorded none; after the Owner mutes the category `talk` sits in and the page reloads, a role mention from a third member recorded none (the same mention recorded one before the mute). One recorded sample: title `bobded_... mentioned you`, body `hello @owner`.

## Docs updated
`docs/05-api-and-gateway.md` (new paragraph before "Entrance copy"), `docs/specs/lounge-chat.md` section 10 (blocked or ignored mention, the order of levels and mutes, what a mention is, replies). The Notification Settings labels ("Suppress @everyone and @here", "Suppress all role @mentions") do not say anything the new behaviour contradicts: no new string key and no dead key. `node scripts/check-doc-links.mjs` run: "doc links ok". Sentences for others: none outside my files, except: `docs/specs/lounge-chat.md` section 10 still says push is unchanged for blocked authors (true).

## Proposed inventory rows (after SEC-AUD-236)
- SEC-AUD-237 [FE] The open app's live mention count follows the server: nothing for a blocked or ignored author, one for a role mention (D-721). Closes the notify handoff's wrong sentence "the server's counts are untouched".
- SEC-AUD-238 [BE] The Inbox's mention list leaves out blocked and ignored authors (D-721).
- SEC-AUD-239 [BE] A reply pings unless the sender turns the ping off; the reply push only when it pinged (D-725).
- SEC-AUD-240 [FE] The desktop notification and sound decide by the server engine's order, mutes and mention kinds; one shared case file (D-727). Open: the person's own default level is not read by the open app.
- SEC-AUD-241 [BE+FE] The suppress-everyone and suppress-roles switches keep the mention badge off, server and live (D-726).

## Proposed threat-model rows
- The Inbox as a place a blocked account's words came back: closed by 238 (blocked either way, ignored).
- The live badge as a way a blocked account could still reach someone: closed for accounts the viewer blocked or ignores; the known rest: an author who blocked the viewer still counts live until a reload.
- A suppressed `@everyone` or role ping as a badge: closed by 241 (the over-count when both kinds are on one message is the only gap).

## Changelog sentence (ops's file)
"A mention from someone you blocked or ignore no longer shows a badge while the app is open, a role mention does, a reply pings the person you replied to unless you turn the ping off (and the push only says "replied to you" when it did), the two Suppress switches now also keep the badge off, and the desktop notification follows the same rules as push (role mentions, category mutes, the server's default)."

## Owner questions
1. The person's own default level: should the open app fetch `/users/@me/notification-defaults` once (and follow `SR_NOTIFICATION_DEFAULTS_UPDATE`) so the desktop notification honours it? Default chosen: no (task says leave it out).
2. With `@everyone` plus a role on one message and only one switch on, the badge may still show for a member who does not hold the role. Fix by passing the recipients' roles into the hook? Default chosen: no.

## Cleanup
Scratch databases `sr_p70_mentions` and `sr_p70_mentions_ui` dropped, `p70-mentions-target` and `p70-mentions-bin` deleted, my stack stopped (no listener on 8481/5481/9181), scratch logs removed from the worktree.

# Wave 60 handoff: guild (SEC-AUD-123..126, D-619..D-622)

Branch `fix/p60-guild` (base 34912d1a). No UI, no migration, no route change (openapi untouched).

## Done (commits)
- `262d15a5` SEC-AUD-123 (D-619): `guild_admin_repo::lock_membership(tx, guild, user)` = `pg_advisory_xact_lock(hashtextextended('membership:<g>:<u>', 0))` (one 64-bit key, no int4 folding of snowflakes). Taken first in `insert_ban`, `join_via_invite`, `join_listed`, `decide_application`. In `decide_application` the lock is taken (approve only) BEFORE the status UPDATE, after a plain read of the applicant: the ban transaction holds the lock and then rejects the same application row, so the other order would deadlock. Other `INSERT INTO guild_members` sites checked: `chat_repo` (creator of a new guild), `apps_repo` (bot add), `admin_repo` (staff) check no bans or are not user acts; left alone (apps_repo/admin_repo are not mine).
- `3a27db11` SEC-AUD-124 (D-620) + D-622 note: `EventBus::subscribe_lagging` (default wraps `subscribe` in `Ok`; `InProcessBus` yields `Err(missed)`; `subscribe` now built on it; `LagTapBus` forwards). Platform pump uses it; on `Err` it warns, drains further immediately-ready gaps (coalesce; an event met while draining is handled after), runs `reconcile_memberships()` on the pump task. Contract test `subscribe_lagging_yields_ok_events`. D-622 comment in `member_list.rs`.
- `d1ebafbe` SEC-AUD-125 (D-621): `announce_interest` takes the event and adds `channel_id` (string or null); module doc and `ScheduledEventKind::UserAdd` doc updated. Also holds the gateway lag tests.
- `66f1ae4b` SEC-AUD-126: `RepoGuildRepository::page_members_with_roles` (trait default filters `page_members` with a scanning loop; Postgres `EXISTS` on `member_roles` via shared `page_members_q`; memory override). `viewers_where_roles`; `fanout_recipients` uses it when `!everyone`. Memory `Store.member_page_reads` counter for the test.

## Tests (all pass)
- sr-events `in_process_subscribe_lagging_reports_the_gap` (+ contract); sr-gateway `a_missed_member_left_is_reconciled_after_lag`, `a_missed_member_joined_is_reconciled_after_lag`, `scheduled_event_user_add_in_a_hidden_channel_is_not_dispatched_to_non_viewers`; sr-api `an_rsvp_dispatch_carries_the_events_channel`; sr-chat `a_role_mention_reads_only_the_roles_members` (+ existing mention test); sr-server `lag_bus`; Postgres (`sr_p60_guild`, `--ignored`): settings_repo and invite_join `a_ban_racing_an_*_never_leaves_a_banned_member`, chat_repo `page_members_with_roles_filters_and_pages`.
- Clippy `-D warnings --all-targets` on sr-events, sr-gateway, sr-server, sr-db, sr-chat, sr-api: clean (before the last test-only edit). `cargo fmt --check` clean.
- Per the owner's mid-run instruction, whole-crate suites were not run (CI does). One full sr-gateway run showed my first lag test failing on a stray PRESENCE_UPDATE; fixed with a drain and the filtered rerun passes. sr-chat --lib ran whole once: 398 pass.
- The first-draft ban test sits in the same file as the 078 test; the SQL key text is duplicated in the tests (the helper is crate-private).

## Not done / notes
- A race approval answers `APPLICANT_BANNED` only when the ban row exists without the ban transaction's application rejection; with the real `insert_ban` (which rejects the application) the approval gets `Ok(None)` (not pending). Either way no member row.
- Scratch DB `sr_p60_guild` left in place (drop at integration).

## Proposed inventory rows
- SEC-AUD-123: A ban and every join of the same (guild, user) are serialized by a transaction advisory lock taken before the ban check (insert_ban, invite join, listed join, application approval). A ban committing during a join can no longer leave a banned member. D-619.
- SEC-AUD-124: The gateway's platform pump learns of lag (`subscribe_lagging`) and reconciles every connected session's guilds against the directory, sending GUILD_DELETE/voice leave or GUILD_CREATE as needed. D-620.
- SEC-AUD-125: GUILD_SCHEDULED_EVENT_USER_ADD/REMOVE carry the event's `channel_id` and follow its visibility. D-621.
- SEC-AUD-126: A role mention reads only the role's holders (`page_members_with_roles`); `@everyone` still pages all members.
- D-622: no row (comment only).

## Proposed threat-model rows
- Authorization state held by an in-memory queue (D-620): the gateway's guild subscription set follows membership events delivered over a bounded broadcast channel. Threat: a dropped MemberLeft/Banned event leaves a removed member subscribed. Mitigation: lag is surfaced and every connected user is reconciled against the directory on the pump; residual: the window between the drop and the reconcile (bounded by pump speed). Multi-node buses must implement `subscribe_lagging` the same way.
- Ban/join race (D-619): check-then-insert on bans vs member insert. Mitigation: per (guild, user) advisory lock in every membership-creating user act and in the ban; residual: system paths (bot add, staff appointment) do not check bans by design.

## Owner questions
None.

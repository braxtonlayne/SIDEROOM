# Handoff: feat/threads-forums (LNG-THR-001..003, LNG-FRM-001)

Worktree: C:/Users/braxt/Desktop/SIDEROOM_wt/threads, branch feat/threads-forums (base integration/phase1).
CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/threads-target
Scratch database: `sr_threads` (left in place; Postgres 17 on localhost:5432, user postgres, password in %TEMP%\sr_pg_pw.txt, never commit it).
Planned ports for live run: sr-server 127.0.0.1:8102, metrics 127.0.0.1:9102. Nothing is running now.

## Goal
Threads (types 10/11/12), thread members, archive/lock/auto-archive, REST, forum/media posts, gateway THREAD_* events.

## DONE and verified
- sr-core: error codes ThreadArchived 50083, ThreadAlreadyCreated 160004, ThreadLocked 160005, MaxActiveThreads 160006, TagRequired 40067; ChatError variants.
- sr-chat `threads/` module: model, rules (pure), access (parent-inherited permissions, private-thread gate), repo trait, ThreadService, in-memory repo, HookChain. `Channel` gained `flags`, `thread`, `forum`. ChatService access/mentions/send are thread-aware (locked thread check). ForumPatch on NewChannel/ChannelPatch (tags get ids in service). `list_guild_channels` excludes threads. Verified: `cargo test -p sr-chat` (206 pass, 35 thread tests), clippy clean for sr-chat.
- sr-db: migration 0110_threads.sql, chat_repo channel select/insert/update/delete cascade (threads die with parent), is_thread_member, message_count decrement on delete, `thread_repo.rs` (PgChatRepo: ThreadRepository). Verified: 5 Postgres tests in crates/sr-db/tests/thread_repo.rs pass against sr_threads (`cargo test -p sr-db --test thread_repo -- --ignored --test-threads=1`, SR_TEST_DATABASE_URL=postgres://postgres:<pw>@localhost:5432/sr_threads).

## IN PROGRESS / state
Tree compiled for sr-chat and sr-db; a full `cargo check --workspace --all-targets` was started when paused, result not confirmed (other crates build Channel via Channel::new only, so likely fine).

## REMAINING
1. sr-events: DomainEvent ThreadCreated/Updated/Deleted(viewers)/MembersUpdated/ThreadMemberUpdated (+topics, tests); fix any exhaustive matches (gateway hub, search_jobs firehose).
2. sr-gateway: to_dispatch for THREAD_CREATE/UPDATE/DELETE/MEMBERS_UPDATE/MEMBER_UPDATE, intents (GUILDS; THREAD_MEMBERS_UPDATE needs GUILD_MEMBERS), GuildDirectory::thread_list_sync + THREAD_LIST_SYNC after GUILD_CREATE; tests in tests.rs style.
3. sr-api `routes/threads/` (new): ThreadsDeps (ThreadService + bus + MessagesDeps for rendering/publishing system messages), DTOs, handlers: POST /channels/{id}/messages/{mid}/threads, POST /channels/{id}/threads (with `message` => forum post: create thread, send starter via MessagesDeps, discard on failure), PUT/DELETE thread-members/{uid|@me}, GET thread-members, GET /guilds/{id}/threads/active, GET archived/public|private, GET /channels/{id}/users/@me/threads/archived/private; utoipa + merge into routes::all/openapi; tower oneshot tests.
4. Thread-aware branches in routes/guilds/channels.rs (PATCH/DELETE on a thread -> ThreadService.modify/delete + events); extend ModifyChannelBody/CreateChannelBody/ChannelObject (dto.rs) with thread_metadata, member_count, message_count (cap 50), applied_tags, owner_id, flags, forum fields (available_tags, default_reaction_emoji, default_sort_order, default_forum_layout, default_thread_rate_limit_per_user, default_auto_archive_duration).
5. routes/messages/deps.rs `MessagesDeps::access`: swap the two overwrite lines for `sr_chat::threads::access::guild_channel_permissions` (same as ChatService::access).
6. sr-server: ThreadsDeps extension + MessageHooks chain (activity hook: record_message -> THREAD_UPDATE on unarchive, THREAD_MEMBERS_UPDATE on auto-join), job `threads.auto_archive` singleton in jobs.rs (calls ThreadService::archive_idle, publishes THREAD_UPDATE), gateway_pg: thread-aware can_view_channel (use guild_channel_permissions), GUILD_CREATE `threads`, THREAD_LIST_SYNC.
7. Live verification script (node, like smoke2.mjs) against sr_threads on 8102; OpenAPI regen (bash: cargo run -q -p sr-server -- openapi > packages/api-client/openapi.json; then pnpm generate); tick inventory LNG-THR-001..003, LNG-FRM-001 only when done; fmt/clippy/test workspace; run ignored PG tests on sr_test (--test-threads=1).

## Decisions / notes for owner
- Thread id = starter message id; type-18 notice posted in parent (public threads), type-21 starter in thread; routes must publish these as MESSAGE_CREATE (ThreadService returns them in StartedThread.messages).
- message_count stored exact, reported capped at 50. Pinned posts (flag 1<<1) exempt from auto-archive.
- `list_guild_channels` now excludes threads: guild search / read-state / templates callers do not see thread messages yet (search owner should switch to a threads-inclusive listing).
- Forum starter-message attachments not planned in first cut.
- Message object does not yet carry a `thread` field (messages owner).

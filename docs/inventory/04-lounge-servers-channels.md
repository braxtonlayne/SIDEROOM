# 04 — Lounge: Servers, Channels, Threads, Forums, Invites

Phase 1. Spec: [lounge-chat](../specs/lounge-chat.md) §5, §7, §8.

## LNG-GLD — Guilds
- [x] **LNG-GLD-001** [DB] guilds, guild_members, roles, member_roles migrations
- [x] **LNG-GLD-002** [BE] Create guild (default @everyone, #general, General voice, categories) → `POST /guilds`
- [x] **LNG-GLD-003** [BE] Create from template (built-in + Discord-compatible template JSON) *(`GET /guilds/templates/builtin`, `POST /guilds/templates/{code}` and `POST /guilds {template}` in one transaction; the wizard still creates the extras client-side until the paired web change lands)*
- [x] **LNG-GLD-004** [BE] Get/modify/delete guild; owner transfer
- [x] **LNG-GLD-005** [BE] Member list/search/get/modify (nick, roles, timeout, voice mute/deaf/move), kick, prune (dry-run count). *(voice mute/deaf/move is `PATCH /guilds/{id}/voice-states/{user_id}`, `crates/sr-api/src/routes/voice.rs` with its tests and the gateway voice tests; the member list, search, get, modify, kick and prune are in `routes/guilds/members.rs` and `prune.rs`)*
- [x] **LNG-GLD-006** [BE] Bans (create with delete-message-seconds, list, remove), bulk ban *(`POST /guilds/{id}/bulk-ban` in `routes/guilds/bans.rs`, test `bulk_ban_bans_what_it_can_and_lists_the_rest` in `crates/sr-api/tests/guilds.rs`)*
- [x] **LNG-GLD-007** [BE] Roles CRUD, positions, hierarchy enforcement (can't manage roles ≥ own top role)
- [x] **LNG-GLD-008** [BE] Guild member join/leave pipeline (system message, welcome screen, onboarding, membership screening `pending`) *(the join transaction adds the member as `pending` when the Community has screening on, posts the type 7 notice in the system channel and dispatches `GUILD_MEMBER_ADD`; `GET /guilds/{id}/member-verification`, `PUT /guilds/{id}/requests/@me`, `POST /guilds/{id}/onboarding-responses`; pending members are read-only in `sr-chat` `perms.rs`; tests in `crates/sr-api/tests/lounge.rs` and `routes/settings/tests.rs`, Postgres in `crates/sr-db/tests/lounge_repo.rs`)*
- [x] **LNG-GLD-009** [GW] GUILD_CREATE (lazy), GUILD_UPDATE, GUILD_DELETE, GUILD_MEMBER_ADD/UPDATE/REMOVE, GUILD_ROLE_*, GUILD_BAN_* *(`hub.rs` `to_dispatch` maps every guild, member, role and ban event; `GUILD_CREATE` carries `presences` and `members` only below the large threshold; tests `guild_admin_events_dispatch_their_payload_objects`, `membership_changes_update_subscriptions`, `guild_deleted_notifies_and_unsubscribes` in `crates/sr-gateway/src/tests.rs`)*
- [x] **LNG-GLD-010** [GW] Lazy member list (op 102 ranges → GUILD_MEMBER_LIST_UPDATE ops SYNC/INSERT/UPDATE/DELETE) *(`crates/sr-gateway/src/member_list.rs` builds and diffs the list, `lists.rs` keeps the subscriptions and refreshes them, op 102 in `conn.rs`; the directory side is `PgGuildDirectory::member_list`; tests in `member_list.rs`, `tests.rs` and `gateway_pg.rs`)*
- [x] **LNG-GLD-011** [BE] Server tag (short tag text + badge icon) shown next to members globally if they opt in *(migration 0620 `guilds.tag/tag_badge` and `accounts.primary_guild_*`; `PATCH /guilds/{id}`, `GET /guilds/tag-badges`, `PATCH /users/@me/primary-guild`; user references carry `primary_guild`; tests `server_tag_is_shown_next_to_members_who_opt_in` and `server_tag_shows_only_for_members_who_chose_it`)*
- [x] **LNG-GLD-012** [BE] Per-server profiles (nick, avatar, banner, bio) *(`PATCH /guilds/{id}/members/@me`, `GET /users/@me/guilds/{id}/member`, member object `avatar/banner/bio`, images scoped to the server in `sr-storage` `MediaKind::MemberAvatar`; test `members_edit_their_own_per_server_profile`)*
- [x] **LNG-GLD-013** [BE] Server Directory: listing request, categories, staff approval state, search API *(migration 0620 `guild_directory`; `routes/directory.rs`: categories, search, join, request/state/withdraw for managers, queue and decisions for staff with `MANAGE_DIRECTORY`; rules in `sr-chat` `settings/directory.rs`; tests `server_directory_listing_approval_search_and_join` and `directory_listings_round_trip_and_filter`)*
- [x] **LNG-GLD-014** [BE] Community enable checks (verification level, explicit filter, rules & updates channels)
- [x] **LNG-GLD-015** [FE] Create Server modal step 1: title, subtitle, "Create My Own", "START FROM A TEMPLATE" list (Gaming, Friends, Study Group, School Club, Local Community, Artists & Creators, Watch Club, Reading Club), "Have an invite already?" + Join a Server *(templates are applied by the client after creation; LNG-GLD-003 is the backend half)*
- [x] **LNG-GLD-016** [FE] Create Server step 2: "Tell us more about your server" (club/community vs me & friends, skip)
- [ ] **LNG-GLD-017** [FE] Create Server step 3: customize (icon upload with crop, server name prefilled "<user>'s server", guidelines note), Back / Create
- [x] **LNG-GLD-018** [FE] Join a Server modal (invite link/code input, examples, "Don't have an invite? Check out the Directory") *(no Directory link yet)*
- [ ] **LNG-GLD-019** [FE] Server header button with name + chevron, banner area (if banner), boost/level indicator
- [x] **LNG-GLD-020** [FE] Server dropdown menu: Server Support, Invite to Server, Server Settings, Create Channel, Create Category, Create Event, App Directory, Notification Settings, Privacy Settings, Edit Per-server Profile, Hide Muted Channels (checkbox), Copy Server Info ▸ (ID, link), Security Actions (mods), Leave Server
- [x] **LNG-GLD-021** [FE] Welcome/empty-server screen: "Welcome to <server>", intro line + Getting Started guide link, checklist cards (Invite your friends, Personalize your server with an icon, Send your first message, Add your first app, Support this server) with completion state *(three of the five cards; apps and support need their own features)*
- [x] **LNG-GLD-022** [FE] Member list panel: role groups with counts, online/offline groups, owner crown, bot tag, activity line, virtualized lazy ranges
- [x] **LNG-GLD-023** [FE] Member context menu: Profile, Mention, Message, Call, Add Note, Change Nickname, Invite to Server ▸, Remove Friend, Ignore, Block, Apps ▸, Roles ▸ (checkbox list), Timeout ▸, Kick, Ban, Transfer Ownership, Copy User ID *Call and Apps are not in the menu: there is no voice and no installed apps yet. Transfer Ownership is offered to the owner only.*
- [x] **LNG-GLD-024** [FE] Timeout modal (durations, reason), Kick modal (reason), Ban modal (delete history window, reason)
- [ ] **LNG-GLD-025** [FE] Privacy Settings modal per server (allow DMs, message requests, activity status, joined-server visibility)
- [ ] **LNG-GLD-026** [FE] Edit Per-server Profile modal
- [x] **LNG-GLD-027** [FE] Leave Server confirm (owner must transfer first)
- [ ] **LNG-GLD-028** [FE] Server Directory page (search, categories, server cards with banner, icon, description, members/online, Join)
- [ ] **LNG-GLD-029** [FE] Server preview / lurk mode for Directory community servers (read-only before joining)
- [ ] **LNG-GLD-030** [FE] Membership screening / rules acceptance modal
- [ ] **LNG-GLD-031** [FE] Onboarding flow (questions, options with channels/roles, default channels) for new members
- [ ] **LNG-GLD-032** [FE] Channels & Roles / Browse Channels page (onboarding-enabled servers)
- [ ] **LNG-GLD-033** [FE] Server Guide / Home tab (Community)
- [ ] **LNG-GLD-034** [FE] Security Actions: pause invites, pause DMs (raid mode)

## LNG-CHN — Channels
- [x] **LNG-CHN-001** [DB] channels, permission_overwrites, channel_recipients migrations
- [x] **LNG-CHN-002** [BE] Create/modify/delete channel (all types), position updates (bulk), sync permissions with category
- [x] **LNG-CHN-003** [BE] Permission overwrite CRUD + effective-permission computation per member
- [x] **LNG-CHN-004** [BE] Announcement channels: follow (creates webhook in target), crosspost/publish, auto-publish feature hook *(`POST /channels/{id}/followers`, `POST /channels/{id}/messages/{id}/crosspost`, `AUTO_PUBLISH` channel flag, text/announcement conversion with `PATCH /channels/{id}` `type`; `routes/settings/follow.rs` and `routes/messages/crosspost.rs`; tests in `routes/settings/tests.rs` and `crates/sr-api/tests/guilds.rs`; attachments are not copied)*
- [x] **LNG-CHN-005** [BE] Slowmode (rate_limit_per_user) enforcement with bypass permissions *(`message_rules::check_slowmode` applied by `ChatService::send_message`; `MANAGE_MESSAGES`, `MANAGE_CHANNELS` and `BYPASS_SLOWMODE` skip it; tests `slowmode` in `message_rules/tests.rs` and `slowmode_returns_retry_after` in `routes/messages/tests.rs`)*
- [x] **LNG-CHN-006** [BE] NSFW / age-restricted channel gate (D-032) *(every message endpoint answers 403 `900011` for an NSFW channel, or a thread of one, until the account has answered the age gate: `MessagesDeps::access` with the `AgeGate`; test `age_restricted_channels_need_the_18_plus_answer`)*
- [x] **LNG-CHN-007** [GW] CHANNEL_CREATE/UPDATE/DELETE, CHANNEL_PINS_UPDATE *(`hub.rs` dispatches `CHANNEL_CREATE/UPDATE/DELETE` to the viewers and `CHANNEL_PINS_UPDATE` with the `GUILDS` intent; tests `rest_message_family_is_dispatched_with_payloads`, `channel_delete_reaches_only_the_producer_computed_viewers`)*
- [x] **LNG-CHN-008** [FE] Channel list: categories (collapse/expand, persist), "+" create channel per category, drag-and-drop reorder & re-parent (managers)
- [x] **LNG-CHN-009** [FE] Channel row: type icon (text #, announcement megaphone, forum, media, voice speaker, stage, rules book, locked variants, NSFW variant), name, unread white text + left pill, mention badge, muted dim, hover icons (create invite, edit channel gear) *(no NSFW icon variant yet)*
- [ ] **LNG-CHN-010** [FE] Voice channel row: connected-user sub-rows (avatar, name, speaking ring, mute/deaf/server-mute icons, camera, LIVE badge), user limit "n/m", voice status text, stream preview on hover
- [ ] **LNG-CHN-011** [FE] Events row at top (count, opens events modal)
- [ ] **LNG-CHN-012** [FE] "Unread mentions" / "New unreads" floating jump pills (top/bottom)
- [x] **LNG-CHN-013** [FE] Channel context menu: Mark As Read, Copy Link, Mute Channel ▸, Notification Settings ▸, Edit Channel, Duplicate Channel, Create Text/Voice Channel, Delete Channel, Copy Channel ID, Invite People
- [x] **LNG-CHN-014** [FE] Category context menu: Mark As Read, Collapse, Collapse All, Mute Category, Notification Settings, Edit Category, Delete Category, Copy ID
- [x] **LNG-CHN-015** [FE] Create Channel modal: type radio (Text, Voice, Forum, Announcement, Stage, Media, Watch Party) with descriptions, name input (auto-slug for text), Private Channel toggle → member/role picker step *(no Watch Party type yet)*
- [x] **LNG-CHN-016** [FE] Create Category modal (name, private toggle)
- [ ] **LNG-CHN-017** [FE] Edit Channel settings: Overview (name, topic, slowmode, age-restricted, announcement toggle, hide after inactivity, default thread archive, voice: bitrate, video quality, user limit, region), Permissions (simple private toggle + advanced roles/members overwrite matrix with ✓/✕/⧸ tri-state per permission, search, "sync with category"), Invites, Integrations (webhooks, followed), Features (feature modules channel scope), Delete Channel *Partial: Invites, Integrations (channel webhooks) and Features (thread defaults) tabs are built. Still open: the announcement toggle (the API now converts text and announcement channels with `PATCH /channels/{id}` `type` 0 or 5), voice settings, per-channel feature-module switches (only /admin/features exists) and followed channels.*
- [ ] **LNG-CHN-018** [FE] Channel header: icon + name, topic (click → topic modal), thread list icon, notification bell menu, pins popout, member list toggle, search, inbox, help
- [ ] **LNG-CHN-019** [FE] Channel welcome header ("Welcome to #channel! This is the start of the #channel channel." + Edit Channel)
- [ ] **LNG-CHN-020** [FE] Pins popout (list, jump, unpin)
- [ ] **LNG-CHN-021** [FE] Read-only/announcement composer state + "Follow" button
- [x] **LNG-CHN-022** [FE] Age-gate interstitial for restricted channels *The answer is saved with POST /sr/age-gate and remembered per device (no call reads it back); the message endpoints answer 403 `900011` until the answer is saved (LNG-CHN-006), so retry the request after `POST /sr/age-gate`.*
- [x] **LNG-CHN-023** [FE] Hide Muted Channels behavior
- [ ] **LNG-CHN-024** [FE] Rules channel + Updates channel badges (Community)

## LNG-THR — Threads
- [x] **LNG-THR-001** [BE] Start thread from message / without message, private threads, join/leave, add/remove member, archive/lock/unarchive, auto-archive job
- [x] **LNG-THR-002** [BE] Active/archived (public/private/joined) listings with pagination
- [x] **LNG-THR-003** [GW] THREAD_CREATE/UPDATE/DELETE/LIST_SYNC/MEMBER_UPDATE/MEMBERS_UPDATE
- [ ] **LNG-THR-004** [FE] Threads list popout (Joined / Active / Archived, search) *Partial: Active and Archived lists with Load more are built (header Threads button). Joined filter and search are not.*
- [ ] **LNG-THR-005** [FE] Thread side pane (header, members, notification menu, close, open full view) *Partial: Side pane with name, Follow/Following, close, messages and composer is built. Members list, notification menu and open-full-view are not.*
- [x] **LNG-THR-006** [FE] Thread starter row under message ("N Messages ›" + last message preview) *Shows the thread name and message count; no last-message preview yet.*
- [ ] **LNG-THR-007** [FE] Threads nested in channel list (joined/active)
- [ ] **LNG-THR-008** [FE] Create Thread composer (name, private toggle, archive duration) *Partial: Create Thread dialog (from a message, or standalone from the plus menu and /thread) takes a name and archive duration. The private-thread toggle is not built.*
- [ ] **LNG-THR-009** [FE] Thread settings (name, slowmode, archive duration, lock, delete)

## LNG-FRM — Forum & media channels
- [x] **LNG-FRM-001** [BE] Forum posts (thread + starter message), tags (available_tags, moderated tags), required tag, default reaction, sort order, layout, guidelines
- [ ] **LNG-FRM-002** [FE] Forum channel view: guidelines banner, "New Post" button, search posts, sort & view (list/gallery), tag filter chips
- [ ] **LNG-FRM-003** [FE] Forum post card (title, tags, author, preview text, media thumbnail, reaction, message count, last activity)
- [ ] **LNG-FRM-004** [FE] New Post composer (title, tags picker, content, attachments)
- [ ] **LNG-FRM-005** [FE] Media channel gallery view (thumbnail grid, filters)
- [ ] **LNG-FRM-006** [FE] Forum settings section (tags editor, default reaction, layout, sort, guidelines)

## LNG-INV — Server invites
- [x] **LNG-INV-001** [DB] invites table
- [x] **LNG-INV-002** [BE] Create invite (max_age, max_uses, temporary, unique), list per guild/channel, delete, accept (`POST /invites/{code}`), get with counts
- [x] **LNG-INV-003** [BE] External-share default single-use (D-022) via `target=external` flag *(`POST /channels/{id}/invites` with `external: true` defaults to a single use, `InviteOptions::external_default`; test `invite_defaults_external_and_overrides`)*
- [x] **LNG-INV-004** [BE] Vanity URL (free) + invite splash *(`GET|PATCH /guilds/{id}/vanity-url`; `GET`/`POST /invites/{code}` fall back to the vanity code; the invite object carries the splash; test `vanity_url_works_like_a_free_invite_that_never_expires`)*
- [x] **LNG-INV-005** [BE] Temporary membership removal on disconnect without roles *(`guild_members.temporary`, `InviteJoinRepository::take_temporary_memberships`, the gateway `user_offline` hook and `TemporaryMembers` with a one minute grace; tests `temporary_members_leave_when_they_go_offline_unless_they_hold_a_role` and `temporary_members_are_dropped_unless_they_hold_a_role`)*
- [ ] **LNG-INV-006** [GW] INVITE_CREATE/DELETE
- [x] **LNG-INV-007** [FE] Invite modal: friend list with "Invite" buttons + search, link field with Copy, "Edit invite link" (expire after, max uses, temporary), default single-use for copy *("Copy link" creates an `external` invite, single use for 7 days, and says so; "Invite friends" DMs use the server defaults of 7 days and unlimited uses, D-182; the pickers edit either; no temporary-membership toggle yet)*
- [ ] **LNG-INV-008** [FE] Invite embed in chat (server card with Join/Joined, expired state)
- [ ] **LNG-INV-009** [FE] `/invite/:code` page (logged-out: only "Log in to accept"; logged-in: Accept)

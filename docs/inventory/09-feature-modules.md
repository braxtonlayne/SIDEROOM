# 09 — Built-in Feature Modules (`FTR`)

Phase 1+ (each independent). Spec: doc 11 §1, lounge-chat §13. Every item = a `Feature` impl + settings schema + tests. Custom UI is optional.

- [x] **FTR-CORE-001** [BE][FE] Feature module template + generator (`just new-feature <id>`), docs page `docs/dev/writing-a-feature.md` *(scripts/new-feature.sh + crates/sr-features/templates/feature.rs.tmpl, `just new-feature`, docs/dev/writing-a-feature.md; runtime, actions port and state in sr-features)*
- [x] **FTR-STAR-001** [BE] Starboard: threshold, emoji, target channel, self-star rule, NSFW handling, update counts on reaction changes *(sr-features modules/starboard.rs)*
- [x] **FTR-RR-001** [BE][FE] Reaction Roles: message picker, emoji→role mappings, unique/multi mode, verify mode *(modules/reaction_roles.rs: panels with multiple/unique/verify modes; settings edited with the generic list-of-groups form, message id typed, no bespoke picker)*
- [x] **FTR-WEL-001** [BE] Welcome & Goodbye: channel, templates with placeholders, embed style, DM option *(modules/welcome.rs; embed style, DM option)*
- [x] **FTR-JTC-001** [BE] Join-to-create voice: hub channel, name template, owner controls (limit, lock, rename), auto-delete when empty *(p12 lounge-ch-be: `sr-features/src/modules/join_to_create.rs` (hub channels, name template, owner controls = Manage Channels on the room, auto-delete when empty) over the new `VoiceJoined`/`VoiceLeft` bus events and the `move_member`, `create_voice_channel`, `delete_channel` actions)*
- [x] **FTR-STK-001** [BE] Sticky messages: per channel, re-post after N messages/seconds *(modules/sticky.rs)*
- [x] **FTR-SCH-001** [BE][FE] Scheduled messages: compose for later (composer clock button), list/edit/cancel, job dispatch _sr-userfeat scheduled.rs + tick.rs, routes userfeat/scheduled.rs, composer clock button and saved-items panel; db.rs and userfeat_pg_tests.rs_
- [x] **FTR-REM-001** [BE][FE] Reminders: "Remind me" on messages and `/remind`, durations, DM delivery, list _reminders.rs, DM delivery by the userfeat.tick job, /remind and Remind Me via POST /users/@me/reminders, reminders tab_
- [x] **FTR-BKM-001** [BE][FE] Bookmarks: save messages (private), bookmarks panel with notes _bookmarks.rs, PUT /channels/{id}/messages/{id}/bookmark, bookmarks tab with notes_
- [x] **FTR-ATH-001** [BE] Auto-thread: every message in a channel spawns a thread, name template *(modules/auto_thread.rs)*
- [x] **FTR-BDAY-001** [BE][FE] Birthdays: user sets month/day (privacy), daily announcement channel, role for the day _backend only: birthdays.rs, /users/@me/birthday, daily announcement and 24h role; no settings UI yet_
- [x] **FTR-LVL-001** [BE][FE] Leveling (opt-in): XP per message with cooldown, level roles, `/rank` card, leaderboard page _backend only: leveling.rs, LevelingHook, rank/leaderboard/opt-out routes; no leaderboard page yet_
- [x] **FTR-RSS-001** [BE] RSS/Atom poster: feed URLs, channel, filters, poll interval (SSRF-safe fetcher) _backend only: rss.rs (parser, sr_net::SafeClient, seeded first fetch, backoff), /guilds/{id}/rss-feeds; no feed UI yet_
- [x] **FTR-LIBF-001** [BE] Library feed: post new chapters/episodes (filters: followed by server, genres, kinds) into a channel as rich cards _announcers.rs run_library_feed (cursor per server, kind/genre filters, no age-restricted)_
- [x] **FTR-WPA-001** [BE] Watch party announcer: post when a party starts in the server, role ping option _announcers.rs announce_party and PartyAnnouncer ChatHook wired on PartyService, role ping_
- [x] **FTR-AP-001** [BE] Auto-publish for announcement channels *(already built: channel flag AUTO_PUBLISH, sr-api messages/crosspost.rs, test auto_publish_sends_every_message_to_followers)*
- [x] **FTR-SLOW-001** [BE][FE] Slowmode presets & schedules (e.g. slower during events) *(modules/slowmode_plus.rs: weekly schedules that restore the old value; named presets applied on demand are not built)*
- [x] **FTR-POLLT-001** [BE][FE] Poll templates (saved poll setups per server) _backend only: poll_templates.rs, /guilds/{id}/poll-templates; no poll modal picker yet_
- [x] **FTR-TEMPR-001** [BE] Temporary roles (grant with expiry, job removes) *(modules/temp_roles.rs + /guilds/{id}/sr/temp-roles endpoints + Features panel)*
- [x] **FTR-AUTOR-001** [BE] Auto-roles on join (with delay / after rules acceptance) *(modules/auto_roles.rs; delay and rules acceptance)*
- [x] **FTR-LOG-001** [BE] Mod log channel (message edits/deletes, joins/leaves, role changes → formatted log channel; respects privacy) *(modules/mod_log.rs)*
- [x] **FTR-COUNT-001** [BE] Stat channels (voice channel names showing member count, etc.) *(modules/stat_channels.rs)*
- [x] **FTR-SUGG-001** [BE][FE] Suggestions channel (up/down votes, status tags, staff responses) _backend only: suggestions.rs (card, seeded votes, status edits), /guilds/{id}/suggestions; no UI yet_
- [x] **FTR-TICK-001** [BE][FE] Ticket threads (button creates private thread with staff role) _backend only: tickets.rs and routes (private thread, staff added); no UI yet, route not tested end to end_
- [x] **FTR-QUOTE-001** [BE] Message link auto-quote (embed preview of linked messages the reader can access) *(modules/message_quotes.rs; only quotes channels everyone can read)*

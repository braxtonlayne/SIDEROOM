# 09 — Built-in Feature Modules (`FTR`)

Phase 1+ (each independent). Spec: doc 11 §1, lounge-chat §13. Every item = a `Feature` impl + settings schema + tests. Custom UI is optional.

- [x] **FTR-CORE-001** [BE][FE] Feature module template + generator (`just new-feature <id>`), docs page `docs/dev/writing-a-feature.md` *(scripts/new-feature.sh + crates/sr-features/templates/feature.rs.tmpl, `just new-feature`, docs/dev/writing-a-feature.md; runtime, actions port and state in sr-features)*
- [x] **FTR-STAR-001** [BE] Starboard: threshold, emoji, target channel, self-star rule, NSFW handling, update counts on reaction changes *(sr-features modules/starboard.rs)*
- [x] **FTR-RR-001** [BE][FE] Reaction Roles: message picker, emoji→role mappings, unique/multi mode, verify mode *(modules/reaction_roles.rs: panels with multiple/unique/verify modes; settings edited with the generic list-of-groups form, message id typed, no bespoke picker)*
- [x] **FTR-WEL-001** [BE] Welcome & Goodbye: channel, templates with placeholders, embed style, DM option *(modules/welcome.rs; embed style, DM option)*
- [ ] **FTR-JTC-001** [BE] Join-to-create voice: hub channel, name template, owner controls (limit, lock, rename), auto-delete when empty
- [x] **FTR-STK-001** [BE] Sticky messages: per channel, re-post after N messages/seconds *(modules/sticky.rs)*
- [ ] **FTR-SCH-001** [BE][FE] Scheduled messages: compose for later (composer clock button), list/edit/cancel, job dispatch
- [ ] **FTR-REM-001** [BE][FE] Reminders: "Remind me" on messages and `/remind`, durations, DM delivery, list
- [ ] **FTR-BKM-001** [BE][FE] Bookmarks: save messages (private), bookmarks panel with notes
- [x] **FTR-ATH-001** [BE] Auto-thread: every message in a channel spawns a thread, name template *(modules/auto_thread.rs)*
- [ ] **FTR-BDAY-001** [BE][FE] Birthdays: user sets month/day (privacy), daily announcement channel, role for the day
- [ ] **FTR-LVL-001** [BE][FE] Leveling (opt-in): XP per message with cooldown, level roles, `/rank` card, leaderboard page
- [ ] **FTR-RSS-001** [BE] RSS/Atom poster: feed URLs, channel, filters, poll interval (SSRF-safe fetcher)
- [ ] **FTR-LIBF-001** [BE] Library feed: post new chapters/episodes (filters: followed by server, genres, kinds) into a channel as rich cards
- [ ] **FTR-WPA-001** [BE] Watch party announcer: post when a party starts in the server, role ping option
- [x] **FTR-AP-001** [BE] Auto-publish for announcement channels *(already built: channel flag AUTO_PUBLISH, sr-api messages/crosspost.rs, test auto_publish_sends_every_message_to_followers)*
- [x] **FTR-SLOW-001** [BE][FE] Slowmode presets & schedules (e.g. slower during events) *(modules/slowmode_plus.rs: weekly schedules that restore the old value; named presets applied on demand are not built)*
- [ ] **FTR-POLLT-001** [BE][FE] Poll templates (saved poll setups per server)
- [x] **FTR-TEMPR-001** [BE] Temporary roles (grant with expiry, job removes) *(modules/temp_roles.rs + /guilds/{id}/sr/temp-roles endpoints + Features panel)*
- [x] **FTR-AUTOR-001** [BE] Auto-roles on join (with delay / after rules acceptance) *(modules/auto_roles.rs; delay and rules acceptance)*
- [x] **FTR-LOG-001** [BE] Mod log channel (message edits/deletes, joins/leaves, role changes → formatted log channel; respects privacy) *(modules/mod_log.rs)*
- [x] **FTR-COUNT-001** [BE] Stat channels (voice channel names showing member count, etc.) *(modules/stat_channels.rs)*
- [ ] **FTR-SUGG-001** [BE][FE] Suggestions channel (up/down votes, status tags, staff responses)
- [ ] **FTR-TICK-001** [BE][FE] Ticket threads (button creates private thread with staff role)
- [x] **FTR-QUOTE-001** [BE] Message link auto-quote (embed preview of linked messages the reader can access) *(modules/message_quotes.rs; only quotes channels everyone can read)*

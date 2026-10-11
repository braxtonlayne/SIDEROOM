# 09 — Built-in Feature Modules (`FTR`)

Phase 1+ (each independent). Spec: doc 11 §1, lounge-chat §13. Every item = a `Feature` impl + settings schema + tests. Custom UI is optional.

- [ ] **FTR-CORE-001** [BE][FE] Feature module template + generator (`just new-feature <id>`), docs page `docs/dev/writing-a-feature.md`
- [ ] **FTR-STAR-001** [BE] Starboard: threshold, emoji, target channel, self-star rule, NSFW handling, update counts on reaction changes
- [ ] **FTR-RR-001** [BE][FE] Reaction Roles: message picker, emoji→role mappings, unique/multi mode, verify mode
- [ ] **FTR-WEL-001** [BE] Welcome & Goodbye: channel, templates with placeholders, embed style, DM option
- [ ] **FTR-JTC-001** [BE] Join-to-create voice: hub channel, name template, owner controls (limit, lock, rename), auto-delete when empty
- [ ] **FTR-STK-001** [BE] Sticky messages: per channel, re-post after N messages/seconds
- [ ] **FTR-SCH-001** [BE][FE] Scheduled messages: compose for later (composer clock button), list/edit/cancel, job dispatch
- [ ] **FTR-REM-001** [BE][FE] Reminders: "Remind me" on messages and `/remind`, durations, DM delivery, list
- [ ] **FTR-BKM-001** [BE][FE] Bookmarks: save messages (private), bookmarks panel with notes
- [ ] **FTR-ATH-001** [BE] Auto-thread: every message in a channel spawns a thread, name template
- [ ] **FTR-BDAY-001** [BE][FE] Birthdays: user sets month/day (privacy), daily announcement channel, role for the day
- [ ] **FTR-LVL-001** [BE][FE] Leveling (opt-in): XP per message with cooldown, level roles, `/rank` card, leaderboard page
- [ ] **FTR-RSS-001** [BE] RSS/Atom poster: feed URLs, channel, filters, poll interval (SSRF-safe fetcher)
- [ ] **FTR-LIBF-001** [BE] Library feed: post new chapters/episodes (filters: followed by server, genres, kinds) into a channel as rich cards
- [ ] **FTR-WPA-001** [BE] Watch party announcer: post when a party starts in the server, role ping option
- [ ] **FTR-AP-001** [BE] Auto-publish for announcement channels
- [ ] **FTR-SLOW-001** [BE][FE] Slowmode presets & schedules (e.g. slower during events)
- [ ] **FTR-POLLT-001** [BE][FE] Poll templates (saved poll setups per server)
- [ ] **FTR-TEMPR-001** [BE] Temporary roles (grant with expiry, job removes)
- [ ] **FTR-AUTOR-001** [BE] Auto-roles on join (with delay / after rules acceptance)
- [ ] **FTR-LOG-001** [BE] Mod log channel (message edits/deletes, joins/leaves, role changes → formatted log channel; respects privacy)
- [ ] **FTR-COUNT-001** [BE] Stat channels (voice channel names showing member count, etc.)
- [ ] **FTR-SUGG-001** [BE][FE] Suggestions channel (up/down votes, status tags, staff responses)
- [ ] **FTR-TICK-001** [BE][FE] Ticket threads (button creates private thread with staff role)
- [ ] **FTR-QUOTE-001** [BE] Message link auto-quote (embed preview of linked messages the reader can access)

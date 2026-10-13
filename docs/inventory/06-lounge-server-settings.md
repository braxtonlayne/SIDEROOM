# 06 — Lounge: Server Settings (`LNG-SSET`)

Phase 1 (Server Support UI: Phase 3). Spec: [lounge-chat](../specs/lounge-chat.md) §11. The sidebar order follows the reference screenshot.

## Shell
- [x] **LNG-SSET-001** [FE] Settings sidebar with group headers (`<SERVER NAME>`, EXPRESSION, PEOPLE, APPS, SIDE ROOM, MODERATION), permission-filtered items, Delete Server at bottom with trash icon
- [x] **LNG-SSET-002** [FE] ESC close button, unsaved-changes bar ("Careful — you have unsaved changes!" Reset / Save Changes), shake on navigation attempt

## Server Profile
- [ ] **LNG-SSET-003** [FE] Header text "Customize how your server appears in invite links and, if enabled, in Server Discovery and Announcement Channel messages"
- [x] **LNG-SSET-004** [FE] Name input
- [ ] **LNG-SSET-005** [FE] Icon: "We recommend an image of at least 512x512." + Change Server Icon (crop modal, GIF allowed free), Remove
- [ ] **LNG-SSET-006** [FE] Banner: 10 preset gradient swatches (dark, pink, red, orange, yellow, purple, blue, teal, green, black) + custom image upload (free)
- [ ] **LNG-SSET-007** [FE] Traits: 5 slots (emoji + short text)
- [x] **LNG-SSET-008** [FE] Description textarea ("How did your server get started? Why should people join?")
- [ ] **LNG-SSET-009** [FE] Live preview card (banner, icon, name, "● N Online ● N Members", "Est. <Mon YYYY>")
- [ ] **LNG-SSET-010** [FE] Invite splash background (free)

## Server Tag
- [ ] **LNG-SSET-011** [FE] Tag text (≤ 4 chars), badge picker (icon set + color), preview next to a member name, "Members can choose to display" toggle
- [ ] **LNG-SSET-012** [BE] Tag adoption by members (User Settings → Profiles) + global display

## Engagement
- [ ] **LNG-SSET-013** [FE] System Messages channel + toggles (random welcome message, sticker reply button, support contribution messages, setup tips)
- [x] **LNG-SSET-014** [FE] Default Notification Settings (All / Only @mentions)
- [ ] **LNG-SSET-015** [FE] Inactive channel (AFK) + timeout
- [ ] **LNG-SSET-016** [FE] Activity feed / widget settings (server widget JSON toggle)

## Server Support (replaces Boost Perks)
- [ ] **LNG-SSET-017** [FE] Current level card, pool this month vs next threshold progress bar, grace-period notice
- [ ] **LNG-SSET-018** [FE] Perks comparison table (L0–L3 per doc 08)
- [ ] **LNG-SSET-019** [FE] Contribute button → amount picker → BTC invoice modal
- [ ] **LNG-SSET-020** [FE] Contributors list (respecting anonymity), "Your contributions" history

## Emoji
- [x] **LNG-SSET-021** [FE] Upload requirements copy, Upload Emoji button (multi-file), slots counters (static/animated)
- [x] **LNG-SSET-022** [FE] Emoji table (image, editable name, uploader avatar, role restriction, delete)

## Stickers
- [ ] **LNG-SSET-023** [FE] Sticker slots grid, Upload modal (preview, name, related emoji, description, file)
- [ ] **LNG-SSET-024** [FE] Edit/delete sticker

## Soundboard
- [ ] **LNG-SSET-025** [FE] Sound list (play, name, emoji, volume, uploader), Upload modal (file, trim to 5s with waveform, name, emoji, volume)
- [ ] **LNG-SSET-026** [BE] Sound trimming/transcode to Opus (`symphonia` decode + `opus` encode)

## Members
- [ ] **LNG-SSET-027** [FE] Member table (avatar/name, member since, joined platform, join method/invite code, roles chips, signals), search, sort, filters (roles, join date, unusual activity), pagination
- [ ] **LNG-SSET-028** [FE] Bulk select → add/remove role, kick, ban
- [x] **LNG-SSET-029** [FE] Prune members modal (inactivity days, include roles, count preview) *(Members, needs Kick Members; wording is "have not sent a message in N days" until last-seen tracking lands (F-02), then Discord's, D-166)*
- [ ] **LNG-SSET-030** [FE] Member detail side drawer (roles, timeout, audit history for this member)

## Roles
- [x] **LNG-SSET-031** [FE] Roles list (drag reorder, member counts, search, Create Role, "Default permissions" @everyone row)
- [ ] **LNG-SSET-032** [FE] Role editor tab Display: name, color (swatches + custom + gradient two-color free), role icon (image or unicode emoji, free), hoist toggle, mentionable toggle
- [x] **LNG-SSET-033** [FE] Role editor tab Permissions: grouped permission list with descriptions (General, Membership, Text, Voice, Apps, Events, Stage, Advanced, **SIDE ROOM**), search, Clear permissions, Administrator warning
- [ ] **LNG-SSET-034** [FE] Role editor tab Links (linked roles: placeholder for app role connections, Phase 6)
- [x] **LNG-SSET-035** [FE] Role editor tab Manage Members (search, add members modal, remove)
- [ ] **LNG-SSET-036** [FE] "View Server As Role" preview mode with exit banner
- [x] **LNG-SSET-037** [FE] Delete role confirm; hierarchy lock icons

## Invites
- [x] **LNG-SSET-038** [FE] Invites table (inviter, code, uses, expires, channel), revoke on hover, Pause Invites toggle

## Access
- [ ] **LNG-SSET-039** [FE] Access mode (invite-only / apply to join / Directory-listed if approved)
- [ ] **LNG-SSET-040** [FE] Verification level radio (None, Low (verified email), Medium (account 5+ min), High (member 10+ min), Highest (2FA))
- [ ] **LNG-SSET-041** [FE] Rules screening editor (rules list, "agree" required)
- [ ] **LNG-SSET-042** [FE] Membership applications (questions form; review queue of applicants with approve/deny)
- [ ] **LNG-SSET-043** [FE] Age-restricted server toggle

## Integrations
- [ ] **LNG-SSET-044** [FE] Overview: Webhooks count, Channels Followed count, Bots and Apps list
- [ ] **LNG-SSET-045** [FE] Webhooks manager (create, avatar, name, channel, URL shown once + Reset URL, delete) *(done except avatar; existing webhooks have no Copy URL because tokens are stored hashed, D-164)*
- [ ] **LNG-SSET-046** [FE] Followed channels list (unfollow)
- [ ] **LNG-SSET-047** [FE] App detail: permissions, command permissions overrides (per role/channel), remove app
- [ ] **LNG-SSET-048** [FE] App Directory ↗ link (opens Developer App Directory, Phase 6)

## SIDE ROOM sections
- [ ] **LNG-SSET-049** [FE] Features (guild-scope feature modules via SchemaForm, search, categories, enable toggles)
- [ ] **LNG-SSET-050** [FE] Extensions & Themes (approved packages browser, enable, configure, server accent color)
- [ ] **LNG-SSET-051** [FE] Watch Parties & Library (who can start parties, default party channel, library feed channel & filters)

## Safety Setup
- [ ] **LNG-SSET-052** [FE] DM & spam protection (filter DMs from members, hide suspicious links, warn on new accounts)
- [ ] **LNG-SSET-053** [FE] Raid protection (join rate alerts, auto-lockdown, CAPTCHA **omitted**: invite-only platform)
- [ ] **LNG-SSET-054** [FE] Explicit media content filter (operator-configurable scanner hook; default off)
- [ ] **LNG-SSET-055** [FE] Require 2FA for moderator actions

## Audit Log
- [ ] **LNG-SSET-056** [BE] Audit log entry writing for all guild mutations (Discord action types) + X-Audit-Log-Reason
- [x] **LNG-SSET-057** [FE] Audit log list (filter by user and action, expandable diff rows, icons per action)

## Bans
- [x] **LNG-SSET-058** [FE] Ban list (search, reason, Revoke Ban modal)

## AutoMod
- [x] **LNG-SSET-059** [BE] AutoMod engine (keyword lists w/ wildcards, regex (Rust `regex`), allow list, preset lists, mention spam, spam heuristic, member profile names) with actions block/custom message/alert channel/timeout
- [ ] **LNG-SSET-060** [GW] AUTO_MODERATION_RULE_*, AUTO_MODERATION_ACTION_EXECUTION
- [x] **LNG-SSET-061** [FE] AutoMod page: rule cards (Block Custom Words ×N, Block Mention Spam, Block Spam Content, Flag words in profiles), rule editor (keywords, regex tester, exemptions roles/channels, actions)
- [ ] **LNG-SSET-062** [FE] AutoMod alert message renderer in alert channel (actions: delete, timeout, kick, ban)

## Community
- [ ] **LNG-SSET-063** [FE] Enable Community wizard (Safety checks → Set up basics (rules channel, updates channel) → Finishing touches (default notifications, remove @everyone moderation perms, agree to guidelines))
- [ ] **LNG-SSET-064** [FE] Community Overview (rules/updates channels, safety notifications channel, description, locale, disable community)
- [ ] **LNG-SSET-065** [FE] Onboarding editor (default channels, questions with options mapping to channels/roles, preview, enable)
- [ ] **LNG-SSET-066** [FE] Server Guide editor (welcome message, new member to-dos, resource pages)
- [ ] **LNG-SSET-067** [FE] Welcome Screen editor (description, up to 5 channel cards with emoji + description)
- [ ] **LNG-SSET-068** [FE] Server Insights (basic: members over time, messages per day, voice minutes, top channels), computed by a daily job
- [ ] **LNG-SSET-069** [FE] Directory listing request (category, tags, description, status: pending/approved/denied + staff note)

## Server Template
- [x] **LNG-SSET-070** [BE] Template snapshot (roles, channels, overwrites, settings), sync, delete, code + link
- [ ] **LNG-SSET-071** [FE] Template page (title, description, Generate, Sync, Preview, Copy link, Delete)

## Delete Server
- [ ] **LNG-SSET-072** [FE] Delete modal (type server name, 2FA code if enabled), owner only
- [ ] **LNG-SSET-073** [BE] Guild deletion → channels/messages to user_deleted (30-day window), members removed, GUILD_DELETE

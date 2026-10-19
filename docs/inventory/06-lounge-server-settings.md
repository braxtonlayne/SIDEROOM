# 06 — Lounge: Server Settings (`LNG-SSET`)

Phase 1 (Server Support UI: Phase 3). Spec: [lounge-chat](../specs/lounge-chat.md) §11. The sidebar order follows the reference screenshot.

## Shell
- [x] **LNG-SSET-001** [FE] Settings sidebar with group headers (`<SERVER NAME>`, EXPRESSION, PEOPLE, APPS, SIDE ROOM, MODERATION), permission-filtered items, Delete Server at bottom with trash icon
- [x] **LNG-SSET-002** [FE] ESC close button, unsaved-changes bar ("Careful — you have unsaved changes!" Reset / Save Changes), shake on navigation attempt

## Server Profile
- [x] **LNG-SSET-003** [FE] Header text "Customize how your server appears in invite links and, if enabled, in Server Discovery and Announcement Channel messages"
- [x] **LNG-SSET-004** [FE] Name input
- [x] **LNG-SSET-005** [FE] Icon: "We recommend an image of at least 512x512." + Change Server Icon (crop modal, GIF allowed free), Remove *(p5: upload, remove and live preview are built; the server crops to a square, so there is no crop modal)* *(p14: icon uploads go through a crop modal (square, 512 px; GIFs skip it), `ImageField.tsx` `crop` prop, overview test)*
- [x] **LNG-SSET-006** [FE] Banner: 10 preset gradient swatches (dark, pink, red, orange, yellow, purple, blue, teal, green, black) + custom image upload (free) *(p10: ten preset gradients drawn on a canvas and uploaded as a PNG, `BannerPresets.tsx`, overview test)*
- [x] **LNG-SSET-007** [FE] Traits: 5 slots (emoji + short text) *(p14 J-07: `guilds.traits` (migration 0962), `sr_profile.traits` on the guild object and the directory card, `PATCH /guilds/{id}/sr/profile`; editor `Traits.tsx` in the Overview, chips `TraitChips.tsx` on the directory card and the per-server profile dialog; tests `guild_traits.rs`, `guilds.rs`, `p14_profile.rs`, `p14profile.test.tsx`; custom-emoji traits are kept and shown, the editor offers Unicode only)*
- [x] **LNG-SSET-008** [FE] Description textarea ("How did your server get started? Why should people join?")
- [x] **LNG-SSET-009** [FE] Live preview card (banner, icon, name, "● N Online ● N Members", "Est. <Mon YYYY>") *(p5: preview card with banner, icon, name and Est. date)* *(p13/p14 verified: the card shows "N Online · N Members" from `with_counts`, `Overview.tsx` (test `overview.test.tsx`); p14 fixed the separator that rendered as "00b7")*
   - p13 backend half built: `GET /guilds/{id}?with_counts=true` adds `approximate_member_count` and `approximate_presence_count` (test `guild_counts_come_with_with_counts_only`); the preview card does not use them yet.
- [x] **LNG-SSET-010** [FE] Invite splash background (free) *(p5)*

## Server Tag
- [x] **LNG-SSET-011** [FE] Tag text (≤ 4 chars), badge picker (icon set + color), preview next to a member name, "Members can choose to display" toggle *(p11: section `tag` in server settings, `settings/server/sections/Tag.tsx` on `GET/PUT/DELETE /guilds/{id}/tag`; 16 glyphs in `lib/serverTag.ts` for badge ids 0 to 15; test `settings/server/__tests__/tag.test.tsx`)*
- [x] **LNG-SSET-012** [BE] Tag adoption by members (User Settings → Profiles) + global display *(p10: `guild_tags` + `user_primary_guilds` (migration 0631), `GET/PUT/DELETE /guilds/{id}/tag`, `GET/PUT /users/@me/primary-guild`; other users see `primary_guild` on message authors and user objects while the tag exists, members may display it and the user is still a member; tests `guild_tags_are_adopted_only_while_members_may_display_them`, `server_tags_are_managed_by_managers_and_adopted_by_members`, `server_tag_adoption_shows_only_for_members_while_allowed`); p11 client: the picker is a card on User Settings > Profiles > Servers (`settings/user/sections/ServerTagCard.tsx`, `GET/PUT /users/@me/primary-guild`, test `settings/user/__tests__/serverTag.test.tsx`); p12 web: the tag is drawn next to author names in chat, in the member list and in the DM list (`lounge/components/ServerTagChip.tsx`, fed by `primary_guild` through `serverTagOf` in `lounge/liveMap.ts`; nothing is drawn when the API omits it, which it does once the server stops members displaying the tag or the member leaves, and `identity_enabled: false` is honoured too); test `lounge/__tests__/p12FeatUi.test.tsx` server tag chip*

## Engagement
- [x] **LNG-SSET-013** [FE] System Messages channel + toggles (random welcome message, sticker reply button, support contribution messages, setup tips) *(p5: system messages channel select is built; the four toggles need server support (no system_channel_flags))* *(p12 lounge-ch-be: the four switches are `system_channel_flags` bits (Overview → System messages); the support contribution message has no bit of its own, bits 16 and 32 are accepted)*
- [x] **LNG-SSET-014** [FE] Default Notification Settings (All / Only @mentions)
- [x] **LNG-SSET-015** [FE] Inactive channel (AFK) + timeout *(p5: stored on the guild; voice is not built, so nothing moves members yet)*
- [x] **LNG-SSET-016** [FE] Activity feed / widget settings (server widget JSON toggle) *(p12 lounge-ch-be: Overview → Server widget, `GET|PATCH /guilds/{id}/widget`)*

## Server Support (replaces Boost Perks)
- [x] **LNG-SSET-017** [FE] Current level card, pool this month vs next threshold progress bar, grace-period notice *(built with Server Support (DON-UI-004): level card, pool progress bar, grace notice, `ServerSupport.tsx`)*
- [x] **LNG-SSET-018** [FE] Perks comparison table (L0–L3 per doc 08) *(p10: `PerksTable` in `ServerSupport.tsx`, support test)*
- [x] **LNG-SSET-019** [FE] Contribute button → amount picker → BTC invoice modal *(built with Server Support: contribute switch, amount picker, invoice modal)*
- [x] **LNG-SSET-020** [FE] Contributors list (respecting anonymity), "Your contributions" history *(p10: top contributors (anonymity as the server sends it) plus "Your contributions" from the invoice list, support test)*

## Emoji
- [x] **LNG-SSET-021** [FE] Upload requirements copy, Upload Emoji button (multi-file), slots counters (static/animated)
- [x] **LNG-SSET-022** [FE] Emoji table (image, editable name, uploader avatar, role restriction, delete)

## Stickers
- [x] **LNG-SSET-023** [FE] Sticker slots grid, Upload modal (preview, name, related emoji, description, file) *(p5 verified: grid with slot counters, upload dialog with preview, name, related emoji (tags), description)*
- [x] **LNG-SSET-024** [FE] Edit/delete sticker *(p5 verified: edit and delete)*

## Soundboard
- [x] **LNG-SSET-025** [FE] Sound list (play, name, emoji, volume, uploader), Upload modal (file, trim to 5s with waveform, name, emoji, volume) *(p14: Soundboard tab in Expressions (`SoundboardTab.tsx`): list, play, rename, volume, upload with name/emoji/volume, delete; a file over 5 s is refused with its length instead of trimmed, no waveform, because the server stores files as uploaded (D-201))*
- [x] **LNG-SSET-026** [BE] Sound handling: stored as uploaded, no Opus transcode *(re-scoped 2026-10-15 by D-201: files (MP3/Ogg/WAV, 512 KiB, 5 s) are validated and stored as uploaded and decoded by the browser; `symphonia` decode only when needed; no libopus. Earlier note: p10: not built. There is no soundboard store or API yet (VOX-SB-001), and the only Opus encoder is the `opus` crate, a binding to the C libopus, which this repo's rustls-only, pure-Rust-first policy (docs/03 section 4, marked with a warning) leaves for an ADR; `symphonia` decode alone is pure Rust. See handoff feat-p10-sset-be)*

## Members
- [x] **LNG-SSET-027** [FE] Member table (avatar/name, member since, joined platform, join method/invite code, roles chips, signals), search, sort, filters (roles, join date, unusual activity), pagination *(p5: search and role filter use the server endpoints, with paging. p10: sort by name or join date over the loaded members. p13 H-37: "Joined via" and "Signals" columns and the "Unusual activity only" filter for MANAGE_GUILD; p14 H-49: "Joined after" / "Joined before" date filter, Members.tsx)*
- [x] **LNG-SSET-028** [FE] Bulk select → add/remove role, kick, ban *(p10: row and page checkboxes, bulk Add/Remove role, Kick, Ban run per member with a partial-failure report, `MemberBulk.tsx`, people test)*
- [x] **LNG-SSET-029** [FE] Prune members modal (inactivity days, include roles, count preview) *(Members, needs Kick Members; wording is "have not sent a message in N days" until last-seen tracking lands (F-02), then Discord's, D-166)*
- [x] **LNG-SSET-030** [FE] Member detail side drawer (roles, timeout, audit history for this member) *(p5: details dialog (username, id, joined, nickname, timeout, roles); no audit history because the audit log cannot filter by target)* *(p14: the details dialog lists the audit entries about the member when the viewer has View Audit Log (`Members.tsx` `MemberHistory`, test `p14communityMore.test.tsx`))*
   - p13 backend half built: `GET /guilds/{id}/audit-logs?target_id=` filters entries about one member (test `audit_log_can_be_filtered_by_target`); the drawer does not show the history yet.

## Roles
- [x] **LNG-SSET-031** [FE] Roles list (drag reorder, member counts, search, Create Role, "Default permissions" @everyone row)
- [x] **LNG-SSET-032** [FE] Role editor tab Display: name, color (swatches + custom + gradient two-color free), role icon (image or unicode emoji, free), hoist toggle, mentionable toggle *(p5: name, colour, role icon (image or emoji), hoist, mentionable are built; the two-colour gradient is not backed)* *(p14 J-08: `roles.secondary_color` / `tertiary_color` (migration 0963), the `colors` object on create and edit with audit keys, the Solid / Gradient / Holographic picker in `Roles.tsx`, and gradient names in messages and the member list (`lib/roleGradient.ts`); tests `role_rules.rs`, `guilds.rs`, `p14profile.test.tsx`, `p14roleGradient.test.tsx`)*
- [x] **LNG-SSET-033** [FE] Role editor tab Permissions: grouped permission list with descriptions (General, Membership, Text, Voice, Apps, Events, Stage, Advanced, **SIDE ROOM**), search, Clear permissions, Administrator warning
- [x] **LNG-SSET-034** [FE] Role editor tab Links (linked roles: placeholder for app role connections, Phase 6) *(p5: placeholder tab)*
- [x] **LNG-SSET-035** [FE] Role editor tab Manage Members (search, add members modal, remove)
- [x] **LNG-SSET-036** [FE] "View Server As Role" preview mode with exit banner *(p5: not built, it needs the channel visibility computation in the Lounge (View Server As Role))* *(p14 J-09: built as a dialog in the Roles section (`ViewAsRole.tsx`, `viewAs.ts`): pick roles, the shared `sr-permissions` WASM engine (`permEngine`) computes which channels they see over the loaded channels and overwrites; a dialog, not a Lounge-wide mode with an exit banner; tests `viewAs.test.ts`, `p14profile.test.tsx`)*
- [x] **LNG-SSET-037** [FE] Delete role confirm; hierarchy lock icons

## Invites
- [x] **LNG-SSET-038** [FE] Invites table (inviter, code, uses, expires, channel), revoke on hover, Pause Invites toggle

## Access
- [x] **LNG-SSET-039** [FE] Access mode (invite-only / apply to join / Directory-listed if approved) *(p14: Community > Access tab summarises invite-only / apply to join / Directory-listed and edits the application form (`CommunityMore.tsx`))*
- [x] **LNG-SSET-040** [FE] Verification level radio (None, Low (verified email), Medium (account 5+ min), High (member 10+ min), Highest (2FA)) *(p5: Moderation, Safety Setup tab)*
- [x] **LNG-SSET-041** [FE] Rules screening editor (rules list, "agree" required) *(p14: Community > Rules Screening tab edits `screening_*` (`CommunityMore.tsx` `RulesTab`); the member side is the join dialog (LNG-GLD-030))*
- [x] **LNG-SSET-042** [FE] Membership applications (questions form; review queue of applicants with approve/deny) *(p10 backend half is built: `GET/PUT /guilds/{id}/sr/applications/form`, `POST /guilds/{id}/sr/applications`, `GET /sr/applications` queue, `POST /sr/applications/{id}/decision`; the editor and queue UI are not)* *(p14: form editor and review queue with approve / reject + reason (`CommunityMore.tsx` `AccessTab`, test `p14communityMore.test.tsx`))*
- [x] **LNG-SSET-043** [FE] Age-restricted server toggle *(p5: Community, Age-restricted server)*

## Integrations
- [x] **LNG-SSET-044** [FE] Overview: Webhooks count, Channels Followed count, Bots and Apps list *(p14: Integrations > Overview shows webhook and followed-channel counts and the bots among the first 1000 members, with Remove (kick) (`IntegrationsOverview.tsx`))*
- [x] **LNG-SSET-045** [FE] Webhooks manager (create, avatar, name, channel, URL shown once + Reset URL, delete) *(done except avatar: the webhook create and modify bodies take no avatar and there is no webhook media kind in `sr-storage`, so it needs a backend change first; existing webhooks have no Copy URL because tokens are stored hashed, D-164)* *(p5: webhook avatar added (WebhookAvatar media kind, create and modify take an avatar); Copy URL stays out by design (D-164))*
- [x] **LNG-SSET-046** [FE] Followed channels list (unfollow) *(p14: Integrations > Followed Channels tab lists channel followers with Unfollow (`IntegrationsOverview.tsx` `FollowedChannels`))*
- [x] **LNG-SSET-047** [FE] App detail: permissions, command permissions overrides (per role/channel), remove app *(p14 J-05, J-10: `IntegrationsOverview.tsx` lists `GET /guilds/{id}/integrations` with Remove (`DELETE`), and `BotCommandPermissions.tsx` is the editor over `PUT /applications/{app}/guilds/{guild}/commands/{command}/permissions` (roles, members, channels, the Everyone and All channels rows); a server manager may write them)*
- [x] **LNG-SSET-048** [FE] App Directory ↗ link (opens Developer App Directory, Phase 6) *(the Integrations > Apps tab is the App Directory (`AppDirectory.tsx`, DEV-BOT-012))*

## SIDE ROOM sections
- [x] **LNG-SSET-049** [FE] Features (guild-scope feature modules via SchemaForm, search, categories, enable toggles) *(p11 `Features.tsx` (SchemaForm per module, sections, enable switch); p14 added the search box)*
- [ ] **LNG-SSET-050** [FE] Extensions & Themes (approved packages browser, enable, configure, server accent color) *(p14 open: there is no approved extension / theme package system in the backend)* *(p14 J-21: EXT phase)*
- [x] **LNG-SSET-051** [FE] Watch Parties & Library (who can start parties, default party channel, library feed channel & filters) *(p14 open: watch parties and the library have no per-server settings in the backend (who can start, default channel, feed channel))* *(p14 J-21: EXT phase)* *p16 lounge: Server Settings > Watch Parties & Library (`sections/WatchLibrary.tsx`, test `p16watchLibrary.test.tsx`): who can start = the Start Watch Party role permission (explained, link to Roles), announce channel and filters = the watch party announcer and library feed modules with their SchemaForm. No new backend; a separate per-server "default party channel" setting was not added.*

## Safety Setup
- [x] **LNG-SSET-052** [FE] DM & spam protection (filter DMs from members, hide suspicious links, warn on new accounts) *(p12 lounge-ch-be: Moderation → DM and spam protection stores the settings. p13 mfa-protect (G-95, D-224): `SocialService::open_dm` routes to Message Requests via `dm_access_protected` (`sr_request_reason`), `hide_suspicious_links` is on the guild object and rendered by `lounge/suspiciousLinks.ts`; tests `sr-chat` social, `suspiciousLinks` in `p13protect.test.tsx`)*
- [x] **LNG-SSET-053** [FE] Raid protection (join rate alerts, auto-lockdown, CAPTCHA **omitted**: invite-only platform) *(p14 partial: the join-rate alert threshold (Community > Overview) and Security Actions > Pause invites are built; auto-lockdown needs a backend job)* *(P15 rest-b: auto-lockdown is built as the join-rate alert plus `raid_auto_pause_invites` (pauses invites for 24 hours, J-02, switch in `lounge/shell/SecurityActionsDialog.tsx`; doc 05 section on incident actions); CAPTCHA stays omitted)*
- [x] **LNG-SSET-054** [FE] Explicit media content filter (operator-configurable scanner hook; default off) *(spoiler-only; scanner hook not built (D-224): upload slots in a pending Message Request get the spoiler flag when a shared server has `explicit_media_filter_dms` on, `SocialService::spoiler_attachments`)*
- [x] **LNG-SSET-055** [FE] Require 2FA for moderator actions *(p12 lounge-ch-be: Moderation → Two-factor authentication for moderation, `POST /guilds/{id}/mfa`, enforced by `MfaGate`. Scope per D-223: p13 mfa-protect puts the gate on every elevated route, `900123` for accounts and bot owners without a second factor; tests `elevated_routes_need_a_second_factor_on_a_2fa_server` (`sr-api --test lounge`) and `developer::mfa_tests`)*

## Audit Log
- [x] **LNG-SSET-056** [BE] Audit log entry writing for all guild mutations (Discord action types) + X-Audit-Log-Reason *(p10: every guild mutation writes an entry (guild, channel, overwrite, role, member, ban, kick, prune, invite, webhook, emoji, sticker, automod, onboarding questions, thread create/update/delete, pin, unpin, message delete by a moderator, bulk delete); `X-Audit-Log-Reason` is scoped per request (`router.rs` audit_reason_scope, `settings/audit.rs` ambient reason). Soundboard, scheduled event and integration actions have no feature to audit yet)*
- [x] **LNG-SSET-057** [FE] Audit log list (filter by user and action, expandable diff rows, icons per action)

## Bans
- [x] **LNG-SSET-058** [FE] Ban list (search, reason, Revoke Ban modal)

## AutoMod
- [x] **LNG-SSET-059** [BE] AutoMod engine (keyword lists w/ wildcards, regex (Rust `regex`), allow list, preset lists, mention spam, spam heuristic, member profile names) with actions block/custom message/alert channel/timeout
- [x] **LNG-SSET-060** [GW] AUTO_MODERATION_RULE_*, AUTO_MODERATION_ACTION_EXECUTION *(p10 verified: SettingsEvent maps to DomainEvent in routes/settings/events.rs, gateway passthrough with intents in sr-gateway hub.rs/protocol.rs; tests in routes/settings/tests.rs)*
- [x] **LNG-SSET-061** [FE] AutoMod page: rule cards (Block Custom Words ×N, Block Mention Spam, Block Spam Content, Flag words in profiles), rule editor (keywords, regex tester, exemptions roles/channels, actions)
- [x] **LNG-SSET-062** [FE] AutoMod alert message renderer in alert channel (actions: delete, timeout, kick, ban) *(p14 J-03 / D-244: a `send_alert_message` action posts a type 24 card from the system user with Delete message, a Timeout select, Kick, Ban and Mark resolved (`routes/settings/alerts.rs`); the web renders it as a message with the buttons (`msg/Components.tsx`, `staffActions.ts`); presses go to `POST /guilds/{id}/alert-actions`)*

## Community
- [x] **LNG-SSET-063** [FE] Enable Community wizard (Safety checks → Set up basics (rules channel, updates channel) → Finishing touches (default notifications, remove @everyone moderation perms, agree to guidelines)) *(p10: three-step wizard `CommunityWizard.tsx`, content test)*
- [x] **LNG-SSET-064** [FE] Community Overview (rules/updates channels, safety notifications channel, description, locale, disable community) *(p5)*
- [x] **LNG-SSET-065** [FE] Onboarding editor (default channels, questions with options mapping to channels/roles, preview, enable) *(p10: interactive preview `OnboardingPreview.tsx` plus the wizard)*
- [x] **LNG-SSET-066** [FE] Server Guide editor (welcome message, new member to-dos, resource pages) *(p10 backend half is built: `GET/PUT /guilds/{id}/sr/guide`; the editor is not)* *(p14: Community > Server Guide tab (`CommunityMore.tsx` `GuideTab`); members read it from the server menu (LNG-GLD-033))*
- [x] **LNG-SSET-067** [FE] Welcome Screen editor (description, up to 5 channel cards with emoji + description) *(p5)*
- [x] **LNG-SSET-068** [FE] Server Insights (basic: members over time, messages per day, voice minutes, top channels), computed by a daily job *(p10 backend half is built: daily job `insights.daily` + `GET /guilds/{id}/sr/insights`; voice minutes stay 0 until voice sessions are persisted; the page is not)* *(p14: Community > Insights tab: totals, per-day members and messages with bars, top channels (`CommunityMore.tsx` `InsightsTab`); voice minutes show 0 until voice sessions are persisted)*
- [x] **LNG-SSET-069** [FE] Directory listing request (category, tags, description, status: pending/approved/denied + staff note) *(p14: Community > Directory tab: request, status badge, staff note, withdraw (`CommunityMore.tsx` `DirectoryTab`))*

## Server Template
- [x] **LNG-SSET-070** [BE] Template snapshot (roles, channels, overwrites, settings), sync, delete, code + link
- [x] **LNG-SSET-071** [FE] Template page (title, description, Generate, Sync, Preview, Copy link, Delete) *(p5: create, sync, copy code and link, preview, delete; public page at /template/:code)*

## Delete Server
- [x] **LNG-SSET-072** [FE] Delete modal (type server name, 2FA code if enabled), owner only *(`DeleteServer.tsx` (type the name, owner only); the second factor is asked by `withReauth` when the server requires 2FA for moderation (D-223, `MfaGate` on the route))*
- [x] **LNG-SSET-073** [BE] Guild deletion → channels/messages to user_deleted (30-day window), members removed, GUILD_DELETE *(p10: soft delete in `guild_admin_repo.rs` + migration 0630: messages to user_deleted with a 30-day purge, members/invites/webhooks/installs removed, GUILD_DELETE emitted; the purge job reaps the guild once no message is left; tests `delete_guild_soft_deletes_messages_and_removes_members`, `reap_removes_deleted_guilds_only_once_their_messages_are_gone`)*

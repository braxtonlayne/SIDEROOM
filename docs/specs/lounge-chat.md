# Spec — Lounge: Chat

Phase 1 · Theme `lounge-*` · Decisions: D-040–D-046, D-010–D-012, D-021–D-022, D-121–D-123 · API: doc 05.

**Rule zero:** when this spec is silent, match Discord's current behavior (Oct 2026). Write the assumption in your PR.

## 1. App shell

```
┌────┬───────────────┬──────────────────────────────────────┬───────────────┐
│ S  │ Sidebar       │ Header (channel name, topic, tools)  │               │
│ e  │ (DM home or   ├──────────────────────────────────────┤  Member list  │
│ r  │ server channel│ Message list (virtualized)           │  / User       │
│ v  │ list)         │                                      │  profile panel│
│ e  │               │                                      │  / Thread or  │
│ r  │               ├──────────────────────────────────────┤  search pane  │
│ s  ├───────────────┤ Composer                             │               │
│    │ User panel    │                                      │               │
└────┴───────────────┴──────────────────────────────────────┴───────────────┘
```

- **Global section bar** (thin, above everything): `SIDE ROOM / The lounge · The cinema · The library`, plus a persistent "voice connected" mini-indicator when in a call (voice persists across sections, D-110). It can be collapsed in settings for a pure-Discord look.
- Desktop window title area: back/forward, inbox, help, and the window title (e.g. "Direct Messages", "Doge's server").

### 1.1 Server rail (72px)
Home button (SIDE ROOM door mark, not Discord's logo) with unread badge · separator · guild icons (unread pill, mention badge, selected pill, hover tooltip, drag to reorder, **folders** with name/color, drag-onto-icon to create a folder) · Staff Server (if member, with a special frame) · `+` Add a Server · Discover (Server Directory) · Download-app button: **hidden** (PWA install prompt instead).

### 1.2 Home sidebar (DMs)
- "Find or start a conversation" (opens Quick Switcher, Ctrl/⌘+K).
- **Friends** · **Message Requests**. **Nitro, Shop, and Quests are removed** (D-040). The slot may show **Support SIDE ROOM** (→ `/support`) if the operator enables it.
- "Direct Messages" header with `+` (create DM / group DM, up to 10 recipients).
- DM rows: avatar with status, name, activity line (e.g. "Glooping my glooper," a game, "Watching Cowboy Bebop"), clan/server-tag chips (e.g. `PoE`, `XBOX`, `Toy`), close button on hover, unread bold, mention badge.

### 1.3 User panel (bottom-left)
Avatar + status, display name, custom-status line (truncated), mic toggle + dropdown (input device, input profile), headphones toggle + dropdown (output device), settings gear. When in voice: a voice-connection panel above it (RTC state, ping, channel name, disconnect, camera, screen share, activity, soundboard, noise-suppression **omitted**, D-045).

## 2. Friends page

Header tabs: **Online · All · Pending · Add Friend** (blurple-filled button), plus the `New Group DM` icon on the right.
- Info banner: "Looking for accounts you've blocked or ignored? Go to settings" (dismissible).
- Search box. List grouped "Online — N": avatar/status, name, server-tag chip, status/activity text, Message button, ⋮ menu (Start Video Call, Start Voice Call, Remove Friend, Ignore, Block…).
- Pending: incoming (accept/ignore) and outgoing (cancel).
- Add Friend: input "You can add friends with their SIDE ROOM username." The user must be on this platform.
- **Active Now** column: cards for friends in voice (members' avatars, "In a Voice Channel," server name), live streams (thumbnail preview, LIVE badge, "Sharing their screen"), games/activities (platform icon), **watching/reading** cards (title, poster, progress, "Join watch party" if one is joinable).

## 3. DM & group DM view

Header: avatar + name, then call, video call, pinned, add-friends-to-DM (creates a group), user-profile toggle, and search (scoped). Right **profile panel** (toggleable): banner (accent color or image), avatar + status, display name, `username • pronouns`, badges, Add Friend / ⋯ (Copy Username, Invite to Server, Add Note, Ignore, Block, Report*), bio (markdown), "Member Since," mutual friends/servers summary, "View Full Profile." Group DMs add: member list, rename, icon, leave, invite link (owner).

Message requests: DMs from non-friends land in Message Requests (per privacy settings) with Accept / Ignore / Block.

## 4. User profile modal ("View Full Profile")

Per the reference screenshot: banner; large avatar with status; display name; `username • pronouns`; badges (Staff, Patron, Contributor, Early Member, App Developer, server tag); buttons **Message**, Add Friend / Friend status, ⋯. Left column: bio; **Member Since**; **Friends Since**; **Connections** list (icon + account name + ↗, optional stats line like "115 Followers · 4,720 Following · 150 Likes"); note field. Right tabs: **Activity** ("<name> doesn't have any activity to share here. So mysterious! Ask them to tell you a secret" + Message button), **Mutual Friends (N)**, **Mutual Servers (N)**, and, new for SIDE ROOM, **Watching & Reading** (respects privacy D-120) and **Reviews**.

**Connections** (User Settings → Connections): OAuth-verified where an open OAuth exists (Discord, GitHub, Spotify, Steam, Twitch, YouTube, Reddit, Roblox, TikTok, and Xbox where the provider offers a usable OAuth API). Unverified "link" connections are clearly marked as unverified. Each connection can be shown or hidden on the profile. Stats lines are shown only when the provider API returns them.

## 5. Servers

### 5.1 Create server modal
"Create Your Server": "Your server is where you and your friends hang out. Make yours and start talking." → **Create My Own**; "START FROM A TEMPLATE": Gaming, Friends, Study Group, School Club, Local Community, Artists & Creators, plus SIDE ROOM templates **Watch Club** (watch-party channels + Cinema feed) and **Reading Club** (Library feed + spoiler threads); approved community templates. "Have an invite already?" → **Join a Server**. Next step: "For a club or community / For me and my friends" → name + icon upload → Create.

### 5.2 New server welcome
Channel `#general` and voice `General` (categories "Text Channels," "Voice Channels"). The empty-state checklist reads "Welcome to <server>": Invite your friends · Personalize your server with an icon · Send your first message · Add your first app · **Support this server** (replaces "Unlock perks for everyone with Boosts"). It also links to a Getting Started guide (our docs).

### 5.3 Server header dropdown
Items (order per reference): **Server Support** (replaces Server Boost, shows level progress) · Invite to Server · Server Settings · Create Channel · Create Category · Create Event · App Directory · Notification Settings · Privacy Settings · Edit Per-server Profile · Hide Muted Channels (checkbox) · Copy Server Info ▸ (Copy Server ID, Copy Server Link) · Leave Server (red, not for the owner). Items show only when the user has the permission. A **Security Actions** (pause invites/DMs) item appears for moderators.

### 5.4 Channel list
Server banner (if set) · server name header ▾ · invite icon · Events row (count) · **Channels & Roles / Browse Channels** (when onboarding is on) · categories (collapse, `+`) · channels by type (icon, name, unread/mention state, muted dimming, settings gear and invite on hover, drag to reorder for managers) · voice channels show connected users (avatars, speaking ring, mute/deaf/stream/video icons, LIVE badge, user limit `3/10`, voice-channel status text) · threads nested under parents · "Unread mentions" floating jump pills.

### 5.5 Channel types (all in scope, D-041)
Text, Announcement (follow/publish), Forum (posts with tags, list/gallery layouts, sort, guidelines, required tags), Media channel, Voice (with text chat in voice), Stage, Category, Threads (public/private/announcement), Rules and Updates channels for Community servers, plus SIDE ROOM **Watch Party** channel (a voice channel variant with a pinned theatre view, see the watch-parties spec).

## 6. Messages

### 6.1 Display
- Cozy (default) and Compact modes. Grouping of consecutive messages (≤ 7 min, same author). Timestamps (hover full date). "NEW" divider. Date dividers. Jump-to-present bar. Unread banner "N new messages since…" with Mark as Read.
- Author: avatar (click → profile popout), display name in highest-role color (gradient roles supported), role icon, server-tag chip, APP/BOT badge, timestamp, "(edited)."
- Content: markdown via `sr-markdown`: bold, italic, underline, strike, spoiler `||`, inline code, code blocks with syntax highlighting (highlight via a Rust `syntect`/`tree-sitter` → WASM, or Shiki), block quotes, headers `#`/`##`/`###`, subtext `-#`, lists, masked links `[text](url)`, timestamps `<t:unix:R>`, mentions (`@user`, `@role`, `#channel`, `@everyone`/`@here`), custom emoji `<:name:id>`, slash-command mentions `</cmd:id>`, guild navigation `<id:customize>`, and **SIDE ROOM mentions** `<cinema:title_id>` / `<library:series_id>`, which render as rich cards.
- Attachments: image/video grid (1–10 media), mosaic layout, lightbox (zoom, download, open original), alt text, spoiler blur, generic file card (icon, name, size, download), audio player, **voice message** (waveform, speed), **expired large file** chip (D-043).
- Embeds: link unfurls (OpenGraph/oEmbed fetched server-side via a Rust fetcher with an SSRF-safe resolver), rich embeds from bots, **YouTube/Twitch/Spotify-style playable embeds** (privacy-enhanced iframes, click to load), and Cinema/Library cards ("Watch together" button).
- Stickers, polls (question, up to 10 answers with emoji, duration, multi-select, live results, end early), components (buttons, select menus) from apps, reactions (emoji + count, super reactions as free burst reactions, tooltip of who reacted, add-reaction picker), reply preview line, forwarded message frame, thread starter ("N messages ›"), system messages (joins, boosts → "contributed to Server Support," pins, thread created, call started/ended, channel follows).

### 6.2 Message actions
Hover toolbar: add reaction, quick reactions (frequently used), reply, forward, create thread, edit (own), ⋯. Context menu: Add Reaction ▸, Edit, Reply, Forward, Create Thread, Copy Text, Apps ▸, Mark Unread, Copy Message Link, Pin/Unpin, Speak Message, Remind Me (feature module), Bookmark (feature module), Delete (own, or with Manage Messages), Report* (disabled until Phase 7), Copy Message ID (Developer Mode). Shift-click shortcuts (delete without confirmation, etc.). Up-arrow edits the last message.

### 6.3 Deletion semantics (D-121/D-122)
Delete → `visibility = user_deleted` → `MESSAGE_DELETE` dispatched (all clients remove it) → purge after 30 days. Bulk delete (moderators) behaves the same. No user-visible "deleted message" stubs (Discord parity).

### 6.4 Composer
Multi-line rich input (Slate- or Lexical-based) with inline emoji/mention rendering · `+` menu (Upload a File, Create Thread, Create Poll, Use Apps, Share a Cinema/Library title) · paste-to-upload · drag-and-drop overlay ("Upload to #channel") · upload queue with per-file edit (rename, spoiler, alt text, remove) · size check against `effective_limits` before upload, with a friendly "Max 30 MB. Patrons can send up to 500 MB" hint · gift slot **removed** · GIF picker (operator-configured provider: Tenor/Klipy/self-hosted, or disabled) · sticker picker · emoji picker (search, categories, server sections, skin tone, frequently used, favorites) · apps/slash-command button · voice message record button (mobile and desktop) · autocomplete popups for `@`, `#`, `:`, `/` · typing indicator ("X is typing…", "Several people are typing…") · slowmode countdown · reply bar · edit mode (Esc to cancel, Enter to save) · character counter near the limit · "You do not have permission to send messages in this channel" state · markdown preview of spoilers.

## 7. Member list
Grouped by hoisted role with counts, then Online and Offline (offline hidden in large servers, Discord parity). Each row: avatar/status, name in role color, activity line, crown for owner, bot/app tag. Click → profile popout (banner, avatar, name, roles with add/remove for managers, note, message box, "View Full Profile"). Right-click → member context menu (Profile, Mention, Message, Call, Add Note, Change Nickname, Invite to Server ▸, Remove Friend, Ignore/Block, Roles ▸, Timeout, Kick, Ban, Transfer Ownership, Copy User ID). Lazy loading in ranges via op 102.

## 8. Threads & forums
Discord parity: threads panel (active/archived, joined), thread view in a side pane or full width, auto-archive durations, private threads (invite-only), thread member list, "N new posts" for forums, forum post creation modal (title, content, tags, media), and forum layouts.

## 9. Search
Search bar in the header ("Search <server>" / "Search <username>"). Filters `from:`, `mentions:`, `has:` (link, embed, file, image, video, sound, sticker, poll), `before:`, `during:`, `after:`, `in:`, `pinned:`, `authorType:`. Results pane with jump-to, sorted by newest/oldest/relevance. Backed by Tantivy, permission-filtered at query time, and excludes deleted messages.

## 10. Notifications
Per server, category, and channel: All / Only @mentions / Nothing, Suppress @everyone/@here, Suppress role mentions, Suppress highlights, Mute (15 min … until I turn it off), Mobile push toggles. Inbox (top-right): Mentions, Unreads, For You tabs. Web Push through VAPID (PWA). Badges and desktop notifications with sound packs (themes may supply sounds).

## 11. Server settings (sidebar per reference)

| Group | Sections |
|---|---|
| `<SERVER NAME>` | **Server Profile** (name, icon, banner color/image, traits ×5, description), **Server Tag** (short tag + badge shown next to members' names), **Engagement** (system messages, default notifications, inactive channel/AFK), **Server Support** (replaces Boost Perks: level, pool, contributors, perks table, "Contribute") |
| Expression | Emoji, Stickers, Soundboard |
| People | Members (search, filters, bulk roles, prune), Roles (list, create, drag order, display/permissions/links/members tabs, "View server as role"), Invites (list, revoke, pause), Access (verification level, rules screening/membership applications, invite-only vs Directory) |
| Apps | Integrations (webhooks, bots, followed channels), App Directory ↗ |
| **SIDE ROOM** | **Features** (feature-module settings at guild scope), **Extensions & Themes** (approved packages), **Watch Parties & Library** (defaults: who can start parties, library feed channels) |
| Moderation | Safety Setup (DM/spam/raid protections, verification), Audit Log, Bans, AutoMod (keyword/regex, spam, mention spam, profile filter; actions block/alert/timeout) |
| — | Enable Community (wizard: safety checks → basics (rules/updates channels) → finish), then sections Overview, Onboarding, Server Guide, Welcome Screen, Server Insights (basic), **Directory listing** (request staff approval, D-012) |
| — | Server Template, **Delete Server** (red, owner only, typed confirmation) |

Server Settings uses Discord's full-screen settings layout with ESC close and a "Careful, you have unsaved changes!" bar.

## 12. User settings (full-screen)

| Group | Sections |
|---|---|
| User Settings | My Account (username, display name, email, phone N/A, password, 2FA, passkeys, account removal: "Delete Account"), Profiles (main + per-server profiles, avatar, decoration, effect, banner, theme colors, bio, pronouns, **profile layout (Phase 6)**), Content & Social (DM filters, message requests, age-restricted toggle, sensitive media filters), Data & Privacy (activity privacy D-120, data export, usage data off by default), Family Center **omitted**, Authorized Apps, Devices, Connections, Clips **omitted** |
| **SIDE ROOM** | **Invites** (credits, open codes, create/revoke, history, buy credit), **Support & Perks** (Patron status, invoices, server contributions, leaderboard visibility), **Activity & Libraries** (history, pause, per-section sharing), **Features** (user-scope feature modules) |
| Billing | **Omitted** (replaced by Support & Perks) |
| App Settings | Appearance (theme light/dark/ash/onyx/sync, **per-section themes**, Theme Store, message display cozy/compact, chat font scaling, zoom, space between groups, saturation), Accessibility, Voice & Video (devices, input mode Voice Activity / **Push to Talk** with keybind and release delay, sensitivity, output volume, video preview, camera, screen share quality defaults, attenuation), Chat (inline media, embeds, emoji, stickers, spoilers, markdown preview), Notifications, Keybinds, Language, Streamer Mode, Advanced (Developer Mode, hardware acceleration) |
| What's New / Log Out | |

## 13. Built-in "no-bot-needed" features (feature modules, doc 11 §1.2)
Each is a separate feature module with a settings schema: Starboard · Reaction Roles · Welcome & Goodbye messages · Join-to-create voice channels · Sticky messages · Scheduled messages · Reminders ("Remind me") · Bookmarks · Auto-thread · Birthday announcements · Leveling (opt-in) · RSS/Atom poster · Library feed (new chapters/episodes posted to a channel) · Watch party announcer · Server-side polls templates · Auto-publish for announcement channels · Slowmode presets.

## 14. Limits (Discord parity defaults, operator-configurable)

| Limit | Default |
|---|---|
| Message length | 2,000 (configurable up to 4,000) |
| Attachments per message | 10 |
| File size | 30 MB (Patron 500 MB; server levels per doc 08) |
| Servers per user | 200 |
| Channels per server | 500 |
| Roles per server | 250 |
| Emoji per server | 250 static + 250 animated |
| Stickers / soundboard sounds | 60 / 48 |
| Members per server | 25,000 (above the account cap, effectively unlimited) |
| Pins per channel | 250 |
| Friends | 1,000 |
| Group DM size | 10 |
| Embeds per message | 10 |
| Reactions distinct per message | 20 |

## 15. Acceptance criteria (selected; full checklist in inventory)
- [ ] A user who knows Discord can create a server, invite a friend, set roles with channel overwrites, and chat, without help.
- [ ] Message list holds 60fps scrolling across 50k messages (virtualized), and loading history paginates 50 at a time.
- [ ] Permission changes take effect for open clients within 1s (recomputed on `GUILD_ROLE_UPDATE` / `CHANNEL_UPDATE`).
- [ ] Deleted messages disappear for everyone instantly, remain readable by high-level staff for 30 days (logged), and are purged afterwards (job test with a clock override).
- [ ] Large attachments become "expired" after 7 days. Normal attachments never expire.
- [ ] `sr-markdown` produces identical ASTs in the Rust tests and the WASM tests (shared fixture corpus ≥ 500 cases).
- [ ] Nitro, Shop, Quests, gifting, and Billing do not appear anywhere.
- [ ] Every built-in feature module can be enabled and configured from Server Settings → Features without code changes in the settings UI.

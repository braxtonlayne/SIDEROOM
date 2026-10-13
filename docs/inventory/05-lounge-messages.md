# 05 — Lounge: Messages, Composer, Expression, Search, Notifications

Phase 1. Spec: [lounge-chat](../specs/lounge-chat.md) §6, §9, §10, §14; doc 09.

## LNG-MSG — Messages (backend)
- [x] **LNG-MSG-001** [DB] messages (partitioned), message_edits, attachments, reactions, poll_votes, read_states migrations
- [x] **LNG-MSG-002** [BE] Create message (content, nonce dedupe, reply reference, attachments by upload id, sticker ids, poll, flags, enforce limits & permissions & slowmode)
- [x] **LNG-MSG-003** [BE] Get messages (before/after/around/limit), get single, permission filtering
- [x] **LNG-MSG-004** [BE] Edit message (author only, content/embeds/flags, store previous in message_edits)
- [x] **LNG-MSG-005** [BE] Delete message (author or MANAGE_MESSAGES) → visibility user_deleted, purge_after = +30d, hold check
- [x] **LNG-MSG-006** [BE] Bulk delete (2–100, ≤ 14 days old, Discord parity)
- [x] **LNG-MSG-007** [BE] Forward message (snapshot into target channel, permission checks)
- [x] **LNG-MSG-008** [BE] Pins (pin/unpin/list, 250 cap), system message on pin
- [ ] **LNG-MSG-009** [BE] Reactions add/remove (own, others with permission, all, per-emoji), list users, burst reactions
- [x] **LNG-MSG-010** [BE] Polls: create, vote/unvote, end, expiry job, results, `MESSAGE_POLL_VOTE_ADD/REMOVE`
- [x] **LNG-MSG-011** [BE] Mentions extraction via `sr-markdown`; @everyone/@here permission gate; role mentionable gate
- [x] **LNG-MSG-012** [BE] Read states: ack message, mention counts, mark guild read, mark unread (set ack to previous)
- [x] **LNG-MSG-013** [BE] Typing indicator endpoint + 10s TTL broadcast
- [ ] **LNG-MSG-014** [BE] Link unfurler: SSRF-safe fetch (deny private IPs, size/time limits), OpenGraph/Twitter cards/oEmbed parse, image proxying, cache by URL, operator allow/deny lists
- [ ] **LNG-MSG-015** [BE] Embed suppression (flag), spoiler handling in embeds
- [ ] **LNG-MSG-016** [BE] System messages (member join with randomized lines, pin, thread created, call start/missed/ended, follow added, server support contribution)
- [ ] **LNG-MSG-017** [BE] Message components storage (buttons/selects) for app messages
- [ ] **LNG-MSG-018** [BE] Crosspost (announcement) publish
- [ ] **LNG-MSG-019** [BE] Voice messages: upload with duration + waveform (computed server-side via `symphonia`), playback URLs
- [ ] **LNG-MSG-020** [GW] MESSAGE_CREATE/UPDATE/DELETE/DELETE_BULK, MESSAGE_REACTION_ADD/REMOVE/REMOVE_ALL/REMOVE_EMOJI, TYPING_START, MESSAGE_ACK
- [ ] **LNG-MSG-021** [JOB] `purge_deleted_messages` (hourly, respects hold_id, purges edits + attachments objects)
- [ ] **LNG-MSG-022** [JOB] `expire_large_attachments` (hourly, marks expired, deletes object, dispatch MESSAGE_UPDATE)
- [ ] **LNG-MSG-023** [TEST] Clock-override tests for 30-day purge and 7-day expiry incl. holds

## LNG-MD — `sr-markdown`
- [x] **LNG-MD-001** [BE][WASM] Parser: bold, italic, underline, strike, spoiler, inline code, code block with lang, block quote (single/multi), headers 1–3, subtext, ordered/unordered lists, masked links, autolinks, escapes
- [x] **LNG-MD-002** [BE][WASM] Mentions & tokens: user, role, channel, everyone/here, custom emoji (static/animated), timestamp styles (t,T,d,D,f,F,R), slash-command mention, guild nav, `<cinema:id>`, `<library:id>`
- [x] **LNG-MD-003** [BE][WASM] Unicode emoji shortcode table (`:smile:` → 😄) + emoji-only "jumbo" detection
- [x] **LNG-MD-004** [BE][WASM] AST → plain text (search/notifications), AST JSON for the React renderer
- [x] **LNG-MD-005** [TEST] Shared fixture corpus (≥ 500 cases) run in Rust and WASM
- [ ] **LNG-MD-006** [FE] React AST renderer + syntax highlighting (lazy-loaded grammar set)

## LNG-MSGUI — Message list UI
- [ ] **LNG-MSGUI-001** [FE] Virtualized message list with bidirectional pagination, scroll anchoring, jump-to-message (highlight flash), jump-to-present bar
- [ ] **LNG-MSGUI-002** [FE] Message group header (avatar, name w/ role color/gradient, role icon, tag chip, APP badge, timestamp) + continuation rows with hover timestamp
- [ ] **LNG-MSGUI-003** [FE] Cozy vs compact display modes
- [x] **LNG-MSGUI-004** [FE] Date dividers, "NEW" unread divider, unread banner with "Mark As Read"
- [ ] **LNG-MSGUI-005** [FE] Reply preview line (avatar, name, snippet, click to jump, "Original message was deleted")
- [ ] **LNG-MSGUI-006** [FE] Forwarded message frame ("Forwarded" + source)
- [x] **LNG-MSGUI-007** [FE] "(edited)" tag with tooltip timestamp
- [x] **LNG-MSGUI-008** [FE] Mention highlight (row tint + left bar) for messages mentioning me
- [ ] **LNG-MSGUI-009** [FE] Image/video attachment grid & mosaic (1–10), aspect handling, blurhash placeholders, spoiler blur w/ click reveal, alt text "ALT" chip
- [x] **LNG-MSGUI-010** [FE] Lightbox/media viewer (zoom, pan, prev/next in message, download, open original, copy)
- [x] **LNG-MSGUI-011** [FE] File attachment card (icon by type, name, size, download), text-file preview (expand)
- [x] **LNG-MSGUI-012** [FE] Audio attachment player
- [x] **LNG-MSGUI-013** [FE] Video attachment inline player
- [x] **LNG-MSGUI-014** [FE] Voice message player (waveform, play/pause, playback speed)
- [x] **LNG-MSGUI-015** [FE] Expired large-file chip ("This file has expired")
- [x] **LNG-MSGUI-016** [FE] Embed renderer (color bar, author, title, description md, fields inline/block, image, thumbnail, footer, timestamp, video/provider)
- [x] **LNG-MSGUI-017** [FE] Click-to-load third-party embeds (YouTube etc.) with privacy notice
- [ ] **LNG-MSGUI-018** [FE] Cinema/Library rich cards (poster, title, meta, Watch/Read, Watch together)
- [ ] **LNG-MSGUI-019** [FE] Server invite embed, message link preview
- [x] **LNG-MSGUI-020** [FE] Sticker render (PNG/APNG/Lottie-free GIF/WebP), sticker tooltip
- [x] **LNG-MSGUI-021** [FE] Poll widget (answers with emoji, vote select, multi, results bars, %, voters list modal, time left, End Poll)
- [ ] **LNG-MSGUI-022** [FE] Components: buttons (styles, link, disabled), select menus (string/user/role/channel), loading state, "interaction failed"
- [ ] **LNG-MSGUI-023** [FE] Reactions row (emoji, count, me-reacted state, burst animation, tooltip of reactors, add-reaction button)
- [x] **LNG-MSGUI-024** [FE] Reactors modal (per-emoji tabs, user list, remove for mods)
- [ ] **LNG-MSGUI-025** [FE] Hover action bar (quick reactions ×3, add reaction, reply, forward, create thread, edit, more)
- [ ] **LNG-MSGUI-026** [FE] Message context menu (Add Reaction ▸, Edit, Reply, Forward, Create Thread, Copy Text, Apps ▸, Mark Unread, Copy Message Link, Pin, Speak Message, Remind Me, Bookmark, Delete, Report (flagged off), Copy Message ID)
- [x] **LNG-MSGUI-027** [FE] Inline edit mode (Esc cancel / Enter save / "escape to cancel • enter to save" hint), empty edit → delete prompt
- [x] **LNG-MSGUI-028** [FE] Delete confirm modal (shift-click bypass), pin confirm modal
- [x] **LNG-MSGUI-029** [FE] Forward modal (destination search, multi-select ≤ 5, optional message)
- [ ] **LNG-MSGUI-030** [FE] System message renderers (join, pin, thread, call, follow, support)
- [x] **LNG-MSGUI-031** [FE] Failed-send state (retry/delete), pending (greyed) state with nonce reconciliation
- [ ] **LNG-MSGUI-032** [FE] Blocked/ignored message collapsing
- [x] **LNG-MSGUI-033** [FE] Typing indicator bar (animated dots, names)
- [ ] **LNG-MSGUI-034** [FE] Speak Message (Web Speech API TTS) + /tts permission

## LNG-CMP — Composer
- [x] **LNG-CMP-002** [FE] Enter to send / Shift+Enter newline, markdown shortcuts, paste plain-text rules
- [x] **LNG-CMP-003** [FE] Autocomplete: `@` users/roles/everyone, `#` channels, `:` emoji (fuzzy), `/` commands (built-in + apps), keyboard navigation
- [ ] **LNG-CMP-004** [FE] "+" menu: Upload a File, Create Thread, Create Poll, Use Apps, Share from Cinema, Share from Library
- [ ] **LNG-CMP-005** [FE] Upload queue tray: per-file thumbnail, edit (filename, spoiler, alt/description), remove; size check with Patron hint; multi-file ≤ 10
- [x] **LNG-CMP-006** [FE] Paste image/file from clipboard; drag-and-drop to composer
- [ ] **LNG-CMP-007** [FE] Upload progress (per file), cancel, resumable for > 64 MB (multipart)
- [x] **LNG-CMP-008** [FE] Create Poll modal (question, answers 2–10 with emoji, duration, allow multiple)
- [x] **LNG-CMP-009** [FE] Reply bar ("Replying to @x", ping toggle @ON/OFF, close)
- [ ] **LNG-CMP-010** [FE] Slowmode countdown, "You do not have permission to send messages in this channel", timeout state
- [x] **LNG-CMP-011** [FE] Character counter near limit + over-limit block (offer "upload as file")
- [ ] **LNG-CMP-012** [FE] Voice message recorder (hold to record on mobile, click on desktop, preview, send)
- [x] **LNG-CMP-013** [FE] Draft persistence per channel (local)
- [x] **LNG-CMP-014** [FE] Up-arrow to edit last message
- [ ] **LNG-CMP-015** [FE] Built-in slash commands: /shrug, /tableflip, /unflip, /me, /spoiler, /nick, /tts, /thread, /msg, /giphy-equivalent (GIF provider), /remind (feature), /poll
- [ ] **LNG-CMP-016** [FE] Composer icons row: GIF, sticker, emoji (gift icon **removed**)
- [ ] **LNG-CMP-001** [FE] Rich composer on Lexical (inline tokens for mentions, emoji, channels, timestamps) *(scheduled after the LNG-MSGUI gaps; the composer stays a textarea through Phase 2, D-174)*

## LNG-EXP — Expression (emoji, stickers, GIFs, soundboard data)
- [ ] **LNG-EXP-001** [DB] emojis, stickers, sticker_packs, soundboard_sounds
- [ ] **LNG-EXP-002** [BE] Guild emoji CRUD (PNG/GIF/WebP ≤ 256 KB, auto-resize 128px, animated), role restrictions
- [ ] **LNG-EXP-003** [BE] Guild stickers CRUD (PNG/APNG/GIF ≤ 512 KB, 320×320, related emoji, description, tags)
- [ ] **LNG-EXP-004** [BE] Default sticker packs (original artwork)
- [ ] **LNG-EXP-005** [BE] GIF provider proxy (Tenor/Klipy API key in admin, or disabled), trending/search/categories, privacy proxy of media
- [ ] **LNG-EXP-006** [GW] GUILD_EMOJIS_UPDATE, GUILD_STICKERS_UPDATE
- [ ] **LNG-EXP-007** [FE] Emoji picker (search, categories sidebar with server icons, frequently used, favorites (right-click), skin tone selector, hover preview footer, "use anywhere" free)
- [ ] **LNG-EXP-008** [FE] Sticker picker (packs, search, server sections)
- [ ] **LNG-EXP-009** [FE] GIF picker (trending categories grid, search, favorites star)
- [ ] **LNG-EXP-010** [FE] Expression picker tabs container (GIFs / Stickers / Emoji)
- [ ] **LNG-EXP-011** [FE] Emoji tooltip/popout on click (name, source server, "Join server" if public)
- [ ] **LNG-EXP-012** [FE] Super/burst reaction picker variant + animations

## LNG-SRC — Search
- [x] **LNG-SRC-001** [BE] Tantivy message index schema (content, author, channel, guild, has-flags, timestamps, pinned, author type, mentions)
- [ ] **LNG-SRC-002** [JOB] Index on create/edit, remove on delete; full reindex command
- [x] **LNG-SRC-003** [BE] Search API (guild & DM/channel scopes) with filters from:, mentions:, has:, before:, during:, after:, in:, pinned:, authorType:; permission filtering; sort; pagination; total count
- [x] **LNG-SRC-004** [FE] Search box with filter autocomplete popover (options list, user/channel pickers, date picker)
- [ ] **LNG-SRC-005** [FE] Results pane (grouped by channel, context lines, Jump, sort New/Old/Relevant, pagination)
- [x] **LNG-SRC-006** [FE] Search history (recent queries, clear)

## LNG-NTF — Notifications & inbox
- [x] **LNG-NTF-001** [BE] Notification settings model (guild/category/channel overrides, mute with expiry, suppress everyone/roles/highlights, mobile push)
- [ ] **LNG-NTF-002** [BE] Notification decision engine (mentions, replies, DMs, keyword highlights, thread follows)
- [ ] **LNG-NTF-003** [BE] Web Push (VAPID) subscription management + delivery (`web-push` crate) with collapse keys
- [ ] **LNG-NTF-004** [FE] Notification Settings modal (server: All / Only @mentions / Nothing; suppress toggles; mobile push; channel overrides list)
- [ ] **LNG-NTF-005** [FE] Channel bell menu (Use Category Default / All / Only @mentions / Nothing, Mute ▸)
- [ ] **LNG-NTF-006** [FE] Inbox popout: Mentions (filters: all servers / include roles / include everyone), Unreads (mark all read per channel), For You (friend requests, library updates, submission decisions, invites redeemed)
- [ ] **LNG-NTF-007** [FE] Desktop notifications (Notification API) + sounds + DND suppression
- [ ] **LNG-NTF-008** [FE] Unread title/favicon badge + PWA Badging API
- [ ] **LNG-NTF-009** [FE] In-app toasts for DMs when not focused on that DM (optional)

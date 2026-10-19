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
- [x] **LNG-MSG-009** [BE] Reactions add/remove (own, others with permission, all, per-emoji), list users, burst reactions *Built: reactions.rs routes and events, tests reactions_lifecycle and reaction_limits_and_permissions in routes/messages/tests.rs.*
- [x] **LNG-MSG-010** [BE] Polls: create, vote/unvote, end, expiry job, results, `MESSAGE_POLL_VOTE_ADD/REMOVE`
- [x] **LNG-MSG-011** [BE] Mentions extraction via `sr-markdown`; @everyone/@here permission gate; role mentionable gate
- [x] **LNG-MSG-012** [BE] Read states: ack message, mention counts, mark guild read, mark unread (set ack to previous)
- [x] **LNG-MSG-013** [BE] Typing indicator endpoint + 10s TTL broadcast
- [x] **LNG-MSG-014** [BE] Link unfurler: SSRF-safe fetch (deny private IPs, size/time limits), OpenGraph/Twitter cards/oEmbed parse, image proxying, cache by URL, operator allow/deny lists *(the proxy is `GET /cdn/external/{hmac}/{url_b64}`; the allow/deny lists are `[unfurl] allow_hosts` / `deny_hosts` in the config. The Admin → Features → Embeds page for them is not built yet.)*
- [x] **LNG-MSG-015** [BE] Embed suppression (flag), spoiler handling in embeds _(p14 misc: suppression is the SUPPRESS_EMBEDS flag (send.rs, rich_jobs.rs, tests in rich_tests.rs); a preview of a link written only inside a spoiler now carries `sr_spoiler` (`sr-markdown` `extract_preview_links_marked`, `rich_jobs::run_unfurl`) and the web hides it behind a reveal button (`msg/Embeds.tsx`); client embeds cannot set it)_
- [x] **LNG-MSG-016** [BE] System messages (member join with randomized lines, pin, thread created, call start/missed/ended, follow added, server support contribution) _(p14 misc: join (type 7), pin, thread created, call and the web's 13 random join lines were built; added follow added (type 12, posted in the receiving channel by `POST /channels/{id}/followers`) and server support (types 8 to 11 by level, posted when a server-level payment is credited; honours the system-channel flag bit 1), see `routes/messages/system.rs`)_
- [x] **LNG-MSG-017** [BE] Message components storage (buttons/selects) for app messages *Built: components are accepted on create and stored in messages.components (sr-chat message.rs, routes/messages/send.rs).*
- [x] **LNG-MSG-018** [BE] Crosspost (announcement) publish *Built: routes/messages/crosspost.rs with auto-publish; tested in routes/settings/tests.rs.*
- [x] **LNG-MSG-019** [BE] Voice messages: upload with duration + waveform (computed server-side via `symphonia`), playback URLs _(p14 misc: `sr-chat/src/voice_meta.rs` walks the Ogg Opus file in pure Rust at attachment finalize and sets the duration (last granule minus pre-skip) and a 1 to 256 byte envelope from the Opus packet sizes; symphonia is not used because it has no Opus decoder and would add a dependency; client-declared values only fill in when the file cannot be read; playback URLs existed)_
- [x] **LNG-MSG-020** [GW] MESSAGE_CREATE/UPDATE/DELETE/DELETE_BULK, MESSAGE_REACTION_ADD/REMOVE/REMOVE_ALL/REMOVE_EMOJI, TYPING_START, MESSAGE_ACK *Built: DomainEvent variants in sr-events event.rs cover create/update/delete/bulk/reactions/typing; MESSAGE_ACK in social/read_state.rs.*
- [x] **LNG-MSG-021** [JOB] `purge_deleted_messages` (hourly, respects hold_id, purges edits + attachments objects) *crates/sr-server/src/jobs.rs purge_deleted_messages now also deletes attachment objects; hold-aware.*
- [x] **LNG-MSG-022** [JOB] `expire_large_attachments` (hourly, marks expired, deletes object, dispatch MESSAGE_UPDATE) *crates/sr-server/src/jobs.rs expire_large_attachments (hourly singleton, deletes object, marks expired, dispatches MESSAGE_UPDATE).*
- [x] **LNG-MSG-023** [TEST] Clock-override tests for 30-day purge and 7-day expiry incl. holds *jobs.rs retention_clock_tests (Postgres, now_ms parameter) cover the 30-day purge and 7-day expiry including holds.*

## LNG-MD — `sr-markdown`
- [x] **LNG-MD-001** [BE][WASM] Parser: bold, italic, underline, strike, spoiler, inline code, code block with lang, block quote (single/multi), headers 1–3, subtext, ordered/unordered lists, masked links, autolinks, escapes
- [x] **LNG-MD-002** [BE][WASM] Mentions & tokens: user, role, channel, everyone/here, custom emoji (static/animated), timestamp styles (t,T,d,D,f,F,R), slash-command mention, guild nav, `<cinema:id>`, `<library:id>`
- [x] **LNG-MD-003** [BE][WASM] Unicode emoji shortcode table (`:smile:` → 😄) + emoji-only "jumbo" detection
- [x] **LNG-MD-004** [BE][WASM] AST → plain text (search/notifications), AST JSON for the React renderer
- [x] **LNG-MD-005** [TEST] Shared fixture corpus (≥ 500 cases) run in Rust and WASM
- [x] **LNG-MD-006** [FE] React AST renderer + syntax highlighting (lazy-loaded grammar set) *Shiki core with the JavaScript regex engine and about 30 lazily loaded grammars (js, ts, rust, python, bash, json, sql, html, css, yaml, and more); other languages stay plain. The build output dropped from 12 MB to 4 MB.*

## LNG-MSGUI — Message list UI
- [x] **LNG-MSGUI-001** [FE] Virtualized message list with bidirectional pagination, scroll anchoring, jump-to-message (highlight flash), jump-to-present bar
- [x] **LNG-MSGUI-002** [FE] Message group header (avatar, name w/ role color/gradient, role icon, tag chip, APP badge, timestamp) + continuation rows with hover timestamp *Gradient renders when a role has colors.secondary_color, which the server does not send yet; the role icon is the role's unicode emoji.*
- [x] **LNG-MSGUI-003** [FE] Cozy vs compact display modes
- [x] **LNG-MSGUI-004** [FE] Date dividers, "NEW" unread divider, unread banner with "Mark As Read"
- [x] **LNG-MSGUI-005** [FE] Reply preview line (avatar, name, snippet, click to jump, "Original message was deleted")
- [x] **LNG-MSGUI-006** [FE] Forwarded message frame ("Forwarded" + source) *The source channel is named only when the viewer can see it.*
- [x] **LNG-MSGUI-007** [FE] "(edited)" tag with tooltip timestamp
- [x] **LNG-MSGUI-008** [FE] Mention highlight (row tint + left bar) for messages mentioning me
- [x] **LNG-MSGUI-009** [FE] Image/video attachment grid & mosaic (1–10), aspect handling, blurhash placeholders, spoiler blur w/ click reveal, alt text "ALT" chip *Placeholders appear for images uploaded by this client (width, height and BlurHash are declared at upload); older uploads have none.*
- [x] **LNG-MSGUI-010** [FE] Lightbox/media viewer (zoom, pan, prev/next in message, download, open original, copy)
- [x] **LNG-MSGUI-011** [FE] File attachment card (icon by type, name, size, download), text-file preview (expand)
- [x] **LNG-MSGUI-012** [FE] Audio attachment player
- [x] **LNG-MSGUI-013** [FE] Video attachment inline player
- [x] **LNG-MSGUI-014** [FE] Voice message player (waveform, play/pause, playback speed)
- [x] **LNG-MSGUI-015** [FE] Expired large-file chip ("This file has expired")
- [x] **LNG-MSGUI-016** [FE] Embed renderer (color bar, author, title, description md, fields inline/block, image, thumbnail, footer, timestamp, video/provider)
- [x] **LNG-MSGUI-017** [FE] Click-to-load third-party embeds (YouTube etc.) with privacy notice
- [x] **LNG-MSGUI-018** [FE] Cinema/Library rich cards (poster, title, meta, Watch/Read, Watch together) *Built: lounge/msg/RichCards.tsx (cinema and library resolvers wired in MessageList: poster/cover, title, meta, Watch / Read, Watch together via the watch-party dialog); test lounge/__tests__/p13RichCards.test.tsx.*
- [x] **LNG-MSGUI-019** [FE] Server invite embed, message link preview *Message links only; Cinema/Library cards are LNG-MSGUI-018.*
- [x] **LNG-MSGUI-020** [FE] Sticker render (PNG/APNG/Lottie-free GIF/WebP), sticker tooltip
- [x] **LNG-MSGUI-021** [FE] Poll widget (answers with emoji, vote select, multi, results bars, %, voters list modal, time left, End Poll)
- [x] **LNG-MSGUI-022** [FE] Components: buttons (styles, link, disabled), select menus (string/user/role/channel), loading state, "interaction failed" *Multi-value selects behave as single; interactions still fail because there is no interaction endpoint.*
- [x] **LNG-MSGUI-023** [FE] Reactions row (emoji, count, me-reacted state, burst animation, tooltip of reactors, add-reaction button) *Burst is the add animation; super reactions are not backed by the server.*
- [x] **LNG-MSGUI-024** [FE] Reactors modal (per-emoji tabs, user list, remove for mods)
- [x] **LNG-MSGUI-025** [FE] Hover action bar (quick reactions ×3, add reaction, reply, forward, create thread, edit, more)
- [x] **LNG-MSGUI-026** [FE] Message context menu (Add Reaction ▸, Edit, Reply, Forward, Create Thread, Copy Text, Apps ▸, Mark Unread, Copy Message Link, Pin, Speak Message, Remind Me, Bookmark, Delete, Report (flagged off), Copy Message ID) *No Apps submenu (no installed apps); Remind Me fires while the app is open.*
- [x] **LNG-MSGUI-027** [FE] Inline edit mode (Esc cancel / Enter save / "escape to cancel • enter to save" hint), empty edit → delete prompt
- [x] **LNG-MSGUI-028** [FE] Delete confirm modal (shift-click bypass), pin confirm modal
- [x] **LNG-MSGUI-029** [FE] Forward modal (destination search, multi-select ≤ 5, optional message)
- [x] **LNG-MSGUI-030** [FE] System message renderers (join, pin, thread, call, follow, support) *The call row has no participants or duration (the server stores none).*
- [x] **LNG-MSGUI-031** [FE] Failed-send state (retry/delete), pending (greyed) state with nonce reconciliation
- [x] **LNG-MSGUI-032** [FE] Blocked/ignored message collapsing
- [x] **LNG-MSGUI-033** [FE] Typing indicator bar (animated dots, names)
- [x] **LNG-MSGUI-034** [FE] Speak Message (Web Speech API TTS) + /tts permission

## LNG-CMP — Composer
- [x] **LNG-CMP-002** [FE] Enter to send / Shift+Enter newline, markdown shortcuts, paste plain-text rules
- [x] **LNG-CMP-003** [FE] Autocomplete: `@` users/roles/everyone, `#` channels, `:` emoji (fuzzy), `/` commands (built-in + apps), keyboard navigation
- [x] **LNG-CMP-004** [FE] "+" menu: Upload a File, Create Thread, Create Poll, Use Apps, Share from Cinema, Share from Library *Use Apps, Share from Cinema and Share from Library are listed but disabled until those features exist.*
- [x] **LNG-CMP-005** [FE] Upload queue tray: per-file thumbnail, edit (filename, spoiler, alt/description), remove; size check with Patron hint; multi-file ≤ 10 *Built: lounge/msg/UploadTray.tsx (thumbnail, rename, spoiler, alt text, remove, per-file progress, max 10) plus the size check against GET /donations/me limits with a Patron hint (uploadLimit.ts); test p13UploadTray.test.tsx. The check warns only, the server decides.*
- [x] **LNG-CMP-006** [FE] Paste image/file from clipboard; drag-and-drop to composer
- [x] **LNG-CMP-007** [FE] Upload progress (per file), cancel, resumable for > 64 MB (multipart) *(p16: `POST /channels/{id}/attachments` returns a `multipart` slot for files over 64 MB, `GET .../attachments/{upload_id}/parts` resumes and `DELETE` aborts, complete assembles from the stored parts, the abandoned-upload sweep aborts leftovers (migration 1030); client `lounge/msg/upload.ts` sends parts three at a time with per-part retry, re-asks the server after a stall and resumes the same file after a reload from a localStorage record; p16 D-271: the file is kept in the IndexedDB ledger (`lib/uploadLedger.ts`, store `chat`, 5 rows, 20 h) and the composer reattaches and continues it after a reload, the localStorage record stays the fallback; tests `uploadLedger.test.ts`, `messages/tests.rs`, `sr-db message_store`, `uploadMultipart.test.ts`)*
- [x] **LNG-CMP-008** [FE] Create Poll modal (question, answers 2–10 with emoji, duration, allow multiple)
- [x] **LNG-CMP-009** [FE] Reply bar ("Replying to @x", ping toggle @ON/OFF, close)
- [x] **LNG-CMP-010** [FE] Slowmode countdown, "You do not have permission to send messages in this channel", timeout state
- [x] **LNG-CMP-011** [FE] Character counter near limit + over-limit block (offer "upload as file")
- [x] **LNG-CMP-012** [FE] Voice message recorder (hold to record on mobile, click on desktop, preview, send) *Only in browsers that can record Ogg (Firefox); the server accepts audio/ogg only, and Chrome/Safari record WebM/MP4 so they do not see the button.*
- [x] **LNG-CMP-013** [FE] Draft persistence per channel (local)
- [x] **LNG-CMP-014** [FE] Up-arrow to edit last message
- [x] **LNG-CMP-015** [FE] Built-in slash commands: /shrug, /tableflip, /unflip, /me, /spoiler, /nick, /tts, /thread, /msg, /giphy-equivalent (GIF provider), /remind (feature), /poll *No /giphy: there is no GIF provider (LNG-EXP-005).*
- [x] **LNG-CMP-016** [FE] Composer icons row: GIF, sticker, emoji (gift icon **removed**)
- [ ] **LNG-CMP-001** [FE] Rich composer on Lexical (inline tokens for mentions, emoji, channels, timestamps) *(scheduled after the LNG-MSGUI gaps; the composer stays a textarea through Phase 2, D-174)*

## LNG-EXP — Expression (emoji, stickers, GIFs, soundboard data)
- [x] **LNG-EXP-001** [DB] emojis, stickers, sticker_packs, soundboard_sounds *Partial: emojis, emoji_roles, stickers (0032) and soundboard_sounds (0640) exist; sticker_packs does not (default packs, EXP-004).* *(p12 lounge-ch-be: `sticker_packs` and `stickers.pack_id` exist (migration 0794); default packs wait for artwork (LNG-EXP-004))*
- [x] **LNG-EXP-002** [BE] Guild emoji CRUD (PNG/GIF/WebP ≤ 256 KB, auto-resize 128px, animated), role restrictions *Built in routes/settings/expressions.rs; uploads are now shrunk to 128 px (animated GIF stays animated, undecodable animated WebP is stored as uploaded).*
- [x] **LNG-EXP-003** [BE] Guild stickers CRUD (PNG/APNG/GIF ≤ 512 KB, 320×320, related emoji, description, tags) *Built in expressions.rs; static PNG/GIF are shrunk to 320 px, APNG and Lottie stored as is.*
- [x] **LNG-EXP-004** [BE] Default sticker packs (original artwork) *(p17 server-b, M-03 / D-289: pack "Side Room Words" (`packages/starter-packs/side-room-words`), installed by `sr-cli package seed`, shown in the picker's Default section, hideable per server)* _(p14 misc: plumbing is complete (migration 0794, `GET /sticker-packs`, signed sticker URLs, picker); no install route yet because the artwork is owner-supplied: packs arrive through the layer-4 package or an operator insert)_
- [x] **LNG-EXP-005** [BE] GIF provider proxy (Tenor/Klipy API key in admin, or disabled), trending/search/categories, privacy proxy of media *routes/gifs.rs: Tenor v2 only, off unless [gif] provider and api_key are set; previews go through the signed image proxy. Klipy is not implemented.*
- [x] **LNG-EXP-006** [GW] GUILD_EMOJIS_UPDATE, GUILD_STICKERS_UPDATE *Built: SettingsEvent::EmojisUpdated/StickersUpdated publish GUILD_EMOJIS_UPDATE and GUILD_STICKERS_UPDATE.*
- [x] **LNG-EXP-007** [FE] Emoji picker (search, categories sidebar with server icons, frequently used, favorites (right-click), skin tone selector, hover preview footer, "use anywhere" free) *lounge/components/EmojiPicker.tsx: frequently used, favorites (right-click), skin tone, server emoji section, hover footer. Server icons in a categories sidebar are not built.*
- [x] **LNG-EXP-008** [FE] Sticker picker (packs, search, server sections) *Current server's stickers with search, and the default packs in a "Default" section above them (p17, M-03).*
- [x] **LNG-EXP-009** [FE] GIF picker (trending categories grid, search, favorites star) *lounge/components/GifPicker.tsx (trending, categories, search, favorites in localStorage).*
- [x] **LNG-EXP-010** [FE] Expression picker tabs container (GIFs / Stickers / Emoji) *The GIF tab explains that no GIF provider is connected.*
- [x] **LNG-EXP-011** [FE] Emoji tooltip/popout on click (name, source server, "Join server" if public) _p13 web: `lounge/msg/CustomEmoji.tsx` (clickable custom emoji drawn from /sr/emojis/{id}/url, popout with name, source server and Join over /directory/{id}/join), wired through the new `customEmoji` resolver in markdown-react; test `p13WebUi2.test.tsx`._
- [x] **LNG-EXP-012** [FE] Super/burst reaction picker variant + animations — won't build (D-231: shim only)

## LNG-SRC — Search
- [x] **LNG-SRC-001** [BE] Tantivy message index schema (content, author, channel, guild, has-flags, timestamps, pinned, author type, mentions)
- [x] **LNG-SRC-002** [JOB] Index on create/edit, remove on delete; full reindex command *Built: sr-server search_jobs.rs (index/remove jobs, search.reindex).*
- [x] **LNG-SRC-003** [BE] Search API (guild & DM/channel scopes) with filters from:, mentions:, has:, before:, during:, after:, in:, pinned:, authorType:; permission filtering; sort; pagination; total count
- [x] **LNG-SRC-004** [FE] Search box with filter autocomplete popover (options list, user/channel pickers, date picker)
- [x] **LNG-SRC-005** [FE] Results pane (grouped by channel, context lines, Jump, sort New/Old/Relevant, pagination)
- [x] **LNG-SRC-006** [FE] Search history (recent queries, clear)

## LNG-NTF — Notifications & inbox
- [x] **LNG-NTF-001** [BE] Notification settings model (guild/category/channel overrides, mute with expiry, suppress everyone/roles/highlights, mobile push)
- [x] **LNG-NTF-002** [BE] Notification decision engine (mentions, replies, DMs, keyword highlights, thread follows) *sr-chat social/notifications.rs decide() plus SocialService::decide_notification; keyword highlights are supported by the engine but there is no per-user keyword list yet.* *(p13: the per-user keyword list is `GET/PUT /users/@me/notification-keywords`, and the push notifier now passes the hits to the engine)*
- [x] **LNG-NTF-003** [BE] Web Push (VAPID) subscription management + delivery (`web-push` crate) with collapse keys *crates/sr-push (VAPID, RFC 8291, SSRF-safe sender, RFC test vector), routes/push.rs, sr-server push_notify.rs fan-out with collapse keys. Own implementation on RustCrypto instead of the web-push crate.*
- [x] **LNG-NTF-004** [FE] Notification Settings modal (server: All / Only @mentions / Nothing; suppress toggles; mobile push; channel overrides list) *NotificationDialog: suppress everyone/roles/highlights, mobile push, channel overrides list with reset.*
- [x] **LNG-NTF-005** [FE] Channel bell menu (Use Category Default / All / Only @mentions / Nothing, Mute ▸) *header/NotifyButton.tsx: Use category default / All / Only @mentions / Nothing plus Mute.*
- [x] **LNG-NTF-006** [FE] Inbox popout: Mentions (filters: all servers / include roles / include everyone), Unreads (mark all read per channel), For You (friend requests, library updates, submission decisions, invites redeemed) *Partial: Mentions (GET /users/@me/mentions with role/everyone/server filters) and Unreads (mark read) are built in lounge/inbox; the For You tab is not.* *Built: lounge/inbox/ForYou.tsx (friend requests with Accept/Decline, followed-series chapters, submission decisions, redeemed invites) beside the existing Mentions and Unreads tabs; test p13Inbox.test.tsx.*
- [x] **LNG-NTF-007** [FE] Desktop notifications (Notification API) + sounds + DND suppression *lounge/shell/desktopNotify.ts: Notification API, Web Audio blips, DND and focus suppression.*
- [x] **LNG-NTF-008** [FE] Unread title/favicon badge + PWA Badging API *pwa/titleBadge.ts (title prefix, favicon dot) plus the existing Badging API hook.*
- [x] **LNG-NTF-009** [FE] In-app toasts for DMs when not focused on that DM (optional) *Built: lounge/shell/DmToasts.tsx (follows the desktop-notification rules, only with the window focused on another channel) and the "Pop-up for direct messages" switch; test p13DmToasts.test.tsx.*

# Handoff: feat/p2-lounge-messages (partial, gates green)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p2-lounge-messages, branch feat/p2-lounge-messages (base integration/phase2). Local commits only, nothing pushed.
Frontend only; no Rust, openapi.json or api-client changes were needed (all endpoints already existed).

## Gates
`pnpm --filter @sideroom/wasm build` first, then in apps/web: `tsc --noEmit`, `eslint .`, `vitest run` (345 tests, 29 files; baseline was 293 / 26), `vite build`, `prettier --check` all pass. No Playwright or live-backend run (see Not verified).

## Done
Store and data seam (additive): store Message now carries attachments, embeds, stickers, poll, snapshots, components, flags, type, mentions. New store actions deleteMessage, pinMessage, votePoll, endPoll, forwardMessage; postMessage takes attachments, poll, reply-ping and flags. Reducer applies MESSAGE_REACTION_ADD/REMOVE/REMOVE_ALL/REMOVE_EMOJI and MESSAGE_POLL_VOTE_ADD/REMOVE (others' reactions were not applied before). LoungeActions gained optional deleteMessage, pinMessage, votePoll, endPoll, forwardMessage, retryMessage, discardMessage; sendMessage has a 4th `extras` argument. GuildPermissions gained optional manageMessages, pinMessages, mentionEveryone. Mock provider implements all of them.

Message list/items (src/lounge/MessageList.tsx, MessageItem.tsx, msg/*): attachment mosaic with spoiler blur and ALT chip, lightbox, file card with text preview, audio, video, voice-message waveform player, expired chip, embeds with click-to-load third-party video and privacy notice, polls (vote, bars, voters modal, End Poll), stickers (signed URL via /guilds/{g}/stickers/{id}/url), forward frame, pin/join system rows, message components (buttons/string selects, "This interaction failed"), invite-link card, mention highlight, deleted-reply and load-original reply states, unread banner with Mark As Read, per-channel scroll restoration, failed-send Retry/Delete, inline edit (Enter/Esc, empty edit prompts delete), delete confirm (Shift bypass), pin confirm, forward dialog (<=5 targets + note), reactors modal with moderator remove, right-click context menu, Speak Message.
Composer (src/lounge/Composer.tsx): @ # : / autocomplete with keyboard nav and wire-token resolution at send, slash commands (/shrug /tableflip /unflip /me /spoiler /poll), "+" menu (Upload a File, Create Poll), upload tray with per-file progress, edit (name, alt, spoiler), cancel, paste and drag-drop, reply bar with @ON/@OFF ping, selection formatting toolbar, over-limit block with "Upload as a file", slowmode countdown, typing indicator with animated dots. Editing no longer goes through the composer.
Search (src/lounge/SearchBox.tsx, searchUi.ts): filter suggestion list (people, channels, has:, dates incl. native date input, pinned:), results grouped by channel, sort Newest/Oldest/Most relevant, paging, recent searches with Clear history.

Inventory ticked: LNG-MSGUI-004, 008, 010-017 (except 009), 020, 021, 024, 027, 028, 029, 031, 033; LNG-CMP-002, 003, 006, 008, 009, 011; LNG-SRC-004, 006.

## Not done / partial (left unticked)
- LNG-MSGUI-001: no "load newer" when viewing a jumped-to window (only Jump To Present). 002: role gradient/icon missing. 003 compact mode. 005 reply avatar. 006 forward source label (snapshot has no source). 009 blurhash (backend sends none). 018 Cinema/Library cards (skipped). 019 message-link preview. 022 user/role/channel selects and loading state. 023 burst animation, reactor tooltip, inline add-reaction button. 025 create-thread button. 026 Mark Unread, Remind Me, Bookmark, Apps, Create Thread. 030 thread/call/follow/support rows. 032 blocked/ignored collapsing. 034 /tts permission.
- LNG-CMP-001 rich editor (kept a textarea; mentions are readable text mapped to tokens at send). 004 other "+" items. 005 Patron size hint. 007 resumable multipart >64 MB (backend only presigns single PUT). 010 timeout state (member timeout not in the store). 012 voice recorder (server accepts audio/ogg only; browsers record webm/mp4). 015 /nick /tts /thread /msg /giphy /remind. 016 GIF and sticker picker buttons.
- LNG-SRC-005 context lines in results. LNG-MD-006 syntax highlighting.
- The shared EmojiPicker has no custom emoji, so custom emoji reactions/picks are not possible yet.

## Not verified
No run against the Rust backend or in a real browser. Wire shapes were taken from crates/sr-api (messages, attachments, rich_model) and covered by MSW tests only.

## Risks / owner decisions
- Presigned upload URLs are S3-style absolute URLs: the bucket needs CORS for PUT from the app origin. The tray sets a Content-Type header on the PUT; if the presign signs headers, that must match.
- Embeds and avatars from third-party hosts are fetched directly (referrer stripped). There is no image proxy (LNG-MSG-014), so remote images reveal viewer IPs. Decide whether to block remote images until a proxy exists.
- Spoiler attachments rely on the Discord `SPOILER_` filename prefix because AttachmentObject does not expose a spoiler flag.
- Pin permission is read as PIN_MESSAGES or MANAGE_MESSAGES from base guild permissions (no channel overwrites).
- Rich editor (Lexical) vs textarea for LNG-CMP-001 is still open.
- A stray file `scratch_unused.txt` was created by mistake at `C:\Users\braxt\Desktop\SIDEROOM_wt\SIDEROOM\scratch_unused.txt` (path escape from a Write call). It is outside this worktree; the permission system blocked inspecting it, so it was left. Please delete it.

## How to resume
cd the worktree; `npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 install --offline --frozen-lockfile`, build wasm, then run the gates from apps/web. New code lives in apps/web/src/lounge/msg/ (renderers, dialogs, upload, autocomplete, slash commands, css) plus searchUi.ts; tests are lounge/__tests__/messages.test.tsx, msgLogic.test.ts, search.test.tsx. Shared-file edits were additive: store/{types,rest,store,reducer}.ts, lounge/{data,live,liveMap,mockData}.ts(x), ChannelView.tsx (Composer ref, drop, editing), SearchBox.tsx.

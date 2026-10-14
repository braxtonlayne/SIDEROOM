# 07 — Lounge: User Settings (`LNG-USET`)

Phase 1 (SIDE ROOM sections fill in as their phases land). Spec: [lounge-chat](../specs/lounge-chat.md) §12.

## Shell
- [x] **LNG-USET-001** [FE] Full-screen settings with search box, group headers (USER SETTINGS, SIDE ROOM, APP SETTINGS), What's New, Log Out (confirm), social links/version footer (search box over names and keywords, What's New dialog, version footer; no social links because the platform has none)
- [x] **LNG-USET-002** [BE] User settings blob (versioned JSON, partial PATCH, `USER_SETTINGS_UPDATE` dispatch across sessions). `GET/PATCH /users/@me/settings`; the dispatch is `SR_USER_SETTINGS_UPDATE` because `USER_SETTINGS_UPDATE` already carries presence (see also `/users/@me/notification-defaults` and `/users/@me/privacy`, LNG-USET-013/014 and LNG-NTF-001)

## My Account
- [x] **LNG-USET-003** [FE] Account card (banner, avatar, display name, username, email (reveal), Edit buttons per field) (banner, avatar, display name and username with an Edit each; email reveal)
- [x] **LNG-USET-004** [FE] Change display name / username (rate-limited) / email (password + verification) modals (display name, username (password re-auth, rate limit shown) and email dialogs)
- [x] **LNG-USET-005** [FE] Password & Authentication: Change Password modal, Enable Authenticator App wizard (QR, secret, verify code, backup codes download), View Backup Codes, Remove 2FA, Security Keys/Passkeys list (add, rename, remove) (Change Password on `POST /sr/auth/password` with optional sign-out of other devices; View Backup Codes is regenerate-only because codes are stored hashed)
- [ ] **LNG-USET-006** [FE] Account Removal: Delete Account (explains permanence, D-029: messages remain, need a new invite to return), confirm with password + 2FA + typed username
   - Card shown with the D-029 wording and a "not available yet" notice: the server has no account deletion (LNG-USET-007).
- [ ] **LNG-USET-007** [BE] Account deletion process (state=deleted, profile scrub, remove relationships/memberships/sessions, revoke open invites, forfeit credits, release username after 30d)

## Profiles
- [x] **LNG-USET-008** [FE] Tabs: Main Profile / Per-server Profiles (server picker) (main profile and per-server tabs; the per-server profile is the nickname only)
- [ ] **LNG-USET-009** [FE] Display name, pronouns, avatar (upload/crop/animated, remove), avatar decoration picker, profile effect picker, banner (image/animated or color), profile theme (two colors), about me (190 chars + markdown), server tag display picker
   - Built: display name, pronouns, avatar and banner (pick, crop, GIF kept as is, remove), accent colour, about me (190). Not built, needs server fields: avatar decorations, profile effects, a two-colour profile theme, server tag display, markdown in about me.
- [x] **LNG-USET-010** [FE] Live preview card (popout style)
- [ ] **LNG-USET-011** [FE] Profile Layout entry ("MySpace mode", Phase 6 → EXT)

## Content & Social
- [ ] **LNG-USET-012** [FE] Sensitive media filters (DMs from friends / others / servers: show, blur, block)
   - Needs a server: no media scanning or per-source filter setting exists. The page says so.
- [ ] **LNG-USET-013** [FE] DM spam filter, message requests toggle, allow DMs from server members (default + per server)
   - Built: message requests and allow DMs from server members on `/users/@me/privacy`. Not built, needs server fields: the DM spam filter and a per-server DM override.
- [x] **LNG-USET-014** [FE] Friend request sources (Everyone, Friends of Friends, Server Members) (Everyone / Friends of friends / Server members)
- [ ] **LNG-USET-015** [FE] Age-restricted content (shows confirmation state, re-confirm flow)
   - Needs a server: no way to read or reset the age confirmation (`POST /sr/age-gate` only records an answer). The page says so.
- [x] **LNG-USET-016** [FE] Blocked & ignored users list (unblock/unignore)

## Data & Privacy
- [ ] **LNG-USET-017** [FE] Activity privacy: share watching & reading (default on, friends + shared servers), per-section toggles, hide age-restricted from activity, pause history
   - Switches built and stored in `user_prefs`; nothing publishes watching/reading activity yet, so they take effect when Cinema and Library share it.
- [ ] **LNG-USET-018** [FE] Display current activity as status, share detected activities
   - Same: stored, not yet read by any activity feature.
- [ ] **LNG-USET-019** [FE] Request my data (status, download link)
   - The Request my data card states that the export builder (LNG-USET-020) does not exist; the button is disabled.
- [ ] **LNG-USET-020** [BE][JOB] Data export package builder (JSON + attachment index, zipped, 7-day link)

## Authorized Apps
- [x] **LNG-USET-021** [FE] OAuth2 grants list (app, scopes, Deauthorize)
   - Placeholder page only: the server has no OAuth2 grants to list or revoke, and the page says so.

## Devices
- [x] **LNG-USET-022** [FE] Current device + other sessions (platform, browser, location (coarse from IP, optional), last active), Log out one / all others (this device first, other sessions with Log out and Log out other devices, trusted devices; no coarse location (the server stores none))

## Connections
- [ ] **LNG-USET-023** [FE] Add connection buttons (providers grid), connected cards (display on profile toggle, show stats toggle, remove)
   - Page built with the Discord status and a read-only list; add/remove/refresh needs LNG-USET-024.
- [ ] **LNG-USET-024** [BE] Connection OAuth flows per provider (Discord, GitHub, Spotify, Steam (OpenID), Twitch, YouTube, Reddit, Xbox/TikTok/Roblox where available) + unverified link connections + periodic stats refresh
   - Discord is login/register only; `intent=link`, an unlink route and the other providers do not exist.

## SIDE ROOM: Invites
- [ ] **LNG-USET-025** [FE] Credits balance card ("You can bring N more people"), eligibility explanations
- [ ] **LNG-USET-026** [FE] Create invite (note field) → code + copy + share link `/join/<code>`
- [ ] **LNG-USET-027** [FE] Open codes list (note, created, Revoke), redeemed list (who joined, when)
- [ ] **LNG-USET-028** [FE] Buy an invite ($20 BTC) → invoice modal
- [ ] **LNG-USET-029** [FE] Frozen/suspicion state notice (generic wording)

## SIDE ROOM: Support & Perks
- [ ] **LNG-USET-030** [FE] Patron status (active until, grace), Become a Patron / Extend (1/3/6/12 months)
- [ ] **LNG-USET-031** [FE] Perks table (free vs Patron)
- [ ] **LNG-USET-032** [FE] Invoices history (status, amount, txid link to configured explorer)
- [ ] **LNG-USET-033** [FE] Server contributions history
- [ ] **LNG-USET-034** [FE] Leaderboard visibility (hide name, hide amount)

## SIDE ROOM: Activity & Libraries
- [ ] **LNG-USET-035** [FE] Watch history & reading history management (clear all, per item), defaults for Cinema (autoplay next, subtitle language, audio language, quality cap, data saver) and Library (default reading mode/direction, background, preload count)

## SIDE ROOM: Features
- [ ] **LNG-USET-036** [FE] User-scope feature modules via SchemaForm

## Appearance
- [x] **LNG-USET-037** [FE] Theme: Light / Dark / Ash / Onyx / Sync with computer; Side Room theme option (Light / Dark / Ash / Onyx / Sync with computer / Side Room)
- [x] **LNG-USET-038** [FE] Per-section themes (Lounge / Cinema / Library / Entrance) pickers (Lounge, Cinema, Library and Entrance pickers (Cinema applies when its shell reads the preference))
- [ ] **LNG-USET-039** [FE] Theme Store entry (approved packages)
   - Theme Store entry not built (there is no theme package store yet).
- [ ] **LNG-USET-040** [FE] Message display (Cozy/Compact), show avatars in compact, chat font scaling, space between message groups, zoom level
   - Built: cozy/compact, chat font scale, space between groups, zoom. Not offered: show avatars in compact (compact has no avatar column).
- [x] **LNG-USET-041** [FE] Allow server theming toggle, show section top bar toggle (allow server theming; the section top bar toggle stays device-local in the Lounge store)

## Accessibility
- [ ] **LNG-USET-042** [FE] Saturation, link decorations, role colors display (name / dot / off), reduced motion (sync), autoplay GIFs/stickers/emoji, contrast, TTS rate, show send-message button, dyslexia-friendly font option
   - Built: saturation, link underline, role colours (name/dot/off), reduced motion, sticker and emoji animation, high contrast, dyslexia-friendly text, send button. Not built: TTS rate (no TTS reader), autoplay of GIFs (no GIF autoplay control in the client).

## Voice & Video
- [x] **LNG-USET-043** [FE] Input/output device selects, input/output volume sliders, mic test with level meter
   - Voice & Video page: owned by the voice client work, not touched here.
- [x] **LNG-USET-044** [FE] Input mode: Voice Activity (auto sensitivity toggle / manual threshold slider with live meter) or Push to Talk (keybind recorder, release delay slider)
- [x] **LNG-USET-045** [FE] Echo cancellation, auto gain toggles (noise suppression intentionally absent, D-045)
- [ ] **LNG-USET-046** [FE] Attenuation (lower others' volume while speaking)
- [ ] **LNG-USET-047** [FE] Camera select + preview, always preview video toggle
- [ ] **LNG-USET-048** [FE] Screen share defaults (resolution/fps within entitlement), share audio default
- [ ] **LNG-USET-049** [FE] Sounds: join/leave, mute/deafen, PTT sounds, incoming ring, soundboard volume _(partial, feat/p5-voice-client: join/leave, mute/deafen and PTT sounds with a volume; incoming ring and soundboard volume wait for calls and the soundboard)_
- [ ] **LNG-USET-050** [FE] Advanced: hardware acceleration hint, QoS, E2EE info + "show privacy codes"

## Chat
- [ ] **LNG-USET-051** [FE] Display images/videos/links previews, inline media size, show embeds, emoji reactions, convert emoticons, show stickers & autocomplete, spoiler display (on click / on hover for mine / always), markdown preview, show compose send button
   - Built: link previews, app embeds, inline attachments, reactions, emoticon conversion, spoilers, remote images. Not built: inline media size, sticker autocomplete and markdown preview toggles.

## Notifications
- [ ] **LNG-USET-052** [FE] Enable desktop notifications (permission request), unread badge, taskbar flash equivalent (title blink), push inactive timeout, sounds per type, reaction notifications, email notifications (digest toggles)
   - Built: default level on `/users/@me/notification-defaults`; desktop permission, sounds, badge, title blink, reactions, push timeout and email digest stored in `user_prefs`. The client does not raise desktop notifications and the server does not deliver Web Push or email digests yet; the page says so.

## Keybinds
- [x] **LNG-USET-053** [FE] Keybind list (defaults read-only), add custom keybinds (PTT, toggle mute/deafen, navigate…), recorder (list with recorder for the Lounge actions; voice keybinds belong to the Voice & Video page)

## Language
- [ ] **LNG-USET-054** [FE] Language picker (i18n framework), timestamp 12/24h format
   - Built: region list and 12/24-hour clock (applied to message times). The language picker waits for the translation framework; the interface is English only.

## Streamer Mode
- [ ] **LNG-USET-055** [FE] Toggle + options (hide personal info, hide invite links, disable sounds/notifications)
   - Streamer Mode not built in this pass.

## Advanced
- [ ] **LNG-USET-056** [FE] Developer Mode (Copy ID items), app test mode for developers, reset UI state
   - Built: developer mode flag (saved; Copy ID items are already always shown) and Reset UI state. Not built: app test mode.

## Removed sections (must not exist)
- [x] **LNG-USET-057** [TEST] Assert no Nitro, Server Boost (Discord), Subscriptions, Gift Inventory, Billing, Family Center, Clips, Quests sections are rendered

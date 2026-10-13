# 07 — Lounge: User Settings (`LNG-USET`)

Phase 1 (SIDE ROOM sections fill in as their phases land). Spec: [lounge-chat](../specs/lounge-chat.md) §12.

## Shell
- [ ] **LNG-USET-001** [FE] Full-screen settings with search box, group headers (USER SETTINGS, SIDE ROOM, APP SETTINGS), What's New, Log Out (confirm), social links/version footer
- [x] **LNG-USET-002** [BE] User settings blob (versioned JSON, partial PATCH, `USER_SETTINGS_UPDATE` dispatch across sessions). `GET/PATCH /users/@me/settings`; the dispatch is `SR_USER_SETTINGS_UPDATE` because `USER_SETTINGS_UPDATE` already carries presence (see also `/users/@me/notification-defaults` and `/users/@me/privacy`, LNG-USET-013/014 and LNG-NTF-001)

## My Account
- [ ] **LNG-USET-003** [FE] Account card (banner, avatar, display name, username, email (reveal), Edit buttons per field)
- [ ] **LNG-USET-004** [FE] Change display name / username (rate-limited) / email (password + verification) modals
- [ ] **LNG-USET-005** [FE] Password & Authentication: Change Password modal, Enable Authenticator App wizard (QR, secret, verify code, backup codes download), View Backup Codes, Remove 2FA, Security Keys/Passkeys list (add, rename, remove)
- [ ] **LNG-USET-006** [FE] Account Removal: Delete Account (explains permanence, D-029: messages remain, need a new invite to return), confirm with password + 2FA + typed username
- [ ] **LNG-USET-007** [BE] Account deletion process (state=deleted, profile scrub, remove relationships/memberships/sessions, revoke open invites, forfeit credits, release username after 30d)

## Profiles
- [ ] **LNG-USET-008** [FE] Tabs: Main Profile / Per-server Profiles (server picker)
- [ ] **LNG-USET-009** [FE] Display name, pronouns, avatar (upload/crop/animated, remove), avatar decoration picker, profile effect picker, banner (image/animated or color), profile theme (two colors), about me (190 chars + markdown), server tag display picker
- [x] **LNG-USET-010** [FE] Live preview card (popout style)
- [ ] **LNG-USET-011** [FE] Profile Layout entry ("MySpace mode", Phase 6 → EXT)

## Content & Social
- [ ] **LNG-USET-012** [FE] Sensitive media filters (DMs from friends / others / servers: show, blur, block)
- [ ] **LNG-USET-013** [FE] DM spam filter, message requests toggle, allow DMs from server members (default + per server)
- [ ] **LNG-USET-014** [FE] Friend request sources (Everyone, Friends of Friends, Server Members)
- [ ] **LNG-USET-015** [FE] Age-restricted content (shows confirmation state, re-confirm flow)
- [x] **LNG-USET-016** [FE] Blocked & ignored users list (unblock/unignore)

## Data & Privacy
- [ ] **LNG-USET-017** [FE] Activity privacy: share watching & reading (default on, friends + shared servers), per-section toggles, hide age-restricted from activity, pause history
- [ ] **LNG-USET-018** [FE] Display current activity as status, share detected activities
- [ ] **LNG-USET-019** [FE] Request my data (status, download link)
- [ ] **LNG-USET-020** [BE][JOB] Data export package builder (JSON + attachment index, zipped, 7-day link)

## Authorized Apps
- [ ] **LNG-USET-021** [FE] OAuth2 grants list (app, scopes, Deauthorize)

## Devices
- [ ] **LNG-USET-022** [FE] Current device + other sessions (platform, browser, location (coarse from IP, optional), last active), Log out one / all others

## Connections
- [ ] **LNG-USET-023** [FE] Add connection buttons (providers grid), connected cards (display on profile toggle, show stats toggle, remove)
- [ ] **LNG-USET-024** [BE] Connection OAuth flows per provider (Discord, GitHub, Spotify, Steam (OpenID), Twitch, YouTube, Reddit, Xbox/TikTok/Roblox where available) + unverified link connections + periodic stats refresh

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
- [ ] **LNG-USET-037** [FE] Theme: Light / Dark / Ash / Onyx / Sync with computer; Side Room theme option
- [ ] **LNG-USET-038** [FE] Per-section themes (Lounge / Cinema / Library / Entrance) pickers
- [ ] **LNG-USET-039** [FE] Theme Store entry (approved packages)
- [ ] **LNG-USET-040** [FE] Message display (Cozy/Compact), show avatars in compact, chat font scaling, space between message groups, zoom level
- [ ] **LNG-USET-041** [FE] Allow server theming toggle, show section top bar toggle

## Accessibility
- [ ] **LNG-USET-042** [FE] Saturation, link decorations, role colors display (name / dot / off), reduced motion (sync), autoplay GIFs/stickers/emoji, contrast, TTS rate, show send-message button, dyslexia-friendly font option

## Voice & Video
- [ ] **LNG-USET-043** [FE] Input/output device selects, input/output volume sliders, mic test with level meter
- [ ] **LNG-USET-044** [FE] Input mode: Voice Activity (auto sensitivity toggle / manual threshold slider with live meter) or Push to Talk (keybind recorder, release delay slider)
- [ ] **LNG-USET-045** [FE] Echo cancellation, auto gain toggles (noise suppression intentionally absent, D-045)
- [ ] **LNG-USET-046** [FE] Attenuation (lower others' volume while speaking)
- [ ] **LNG-USET-047** [FE] Camera select + preview, always preview video toggle
- [ ] **LNG-USET-048** [FE] Screen share defaults (resolution/fps within entitlement), share audio default
- [ ] **LNG-USET-049** [FE] Sounds: join/leave, mute/deafen, PTT sounds, incoming ring, soundboard volume
- [ ] **LNG-USET-050** [FE] Advanced: hardware acceleration hint, QoS, E2EE info + "show privacy codes"

## Chat
- [ ] **LNG-USET-051** [FE] Display images/videos/links previews, inline media size, show embeds, emoji reactions, convert emoticons, show stickers & autocomplete, spoiler display (on click / on hover for mine / always), markdown preview, show compose send button

## Notifications
- [ ] **LNG-USET-052** [FE] Enable desktop notifications (permission request), unread badge, taskbar flash equivalent (title blink), push inactive timeout, sounds per type, reaction notifications, email notifications (digest toggles)

## Keybinds
- [ ] **LNG-USET-053** [FE] Keybind list (defaults read-only), add custom keybinds (PTT, toggle mute/deafen, navigate…), recorder

## Language
- [ ] **LNG-USET-054** [FE] Language picker (i18n framework), timestamp 12/24h format

## Streamer Mode
- [ ] **LNG-USET-055** [FE] Toggle + options (hide personal info, hide invite links, disable sounds/notifications)

## Advanced
- [ ] **LNG-USET-056** [FE] Developer Mode (Copy ID items), app test mode for developers, reset UI state

## Removed sections (must not exist)
- [x] **LNG-USET-057** [TEST] Assert no Nitro, Server Boost (Discord), Subscriptions, Gift Inventory, Billing, Family Center, Clips, Quests sections are rendered

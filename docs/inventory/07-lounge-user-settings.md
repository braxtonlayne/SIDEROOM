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
- [x] **LNG-USET-007** [BE] Account deletion process (state=deleted, profile scrub, remove relationships/memberships/sessions, revoke open invites, forfeit credits, release username after 30d) *(POST /sr/auth/account/delete; sr-db account_deletion.rs, migration 0660; tests account_deletion.rs and auth tests)*

## Profiles
- [x] **LNG-USET-008** [FE] Tabs: Main Profile / Per-server Profiles (server picker) (main profile and per-server tabs; the per-server profile is the nickname only)
- [ ] **LNG-USET-009** [FE] Display name, pronouns, avatar (upload/crop/animated, remove), avatar decoration picker, profile effect picker, banner (image/animated or color), profile theme (two colors), about me (190 chars + markdown), server tag display picker
   - Built: display name, pronouns, avatar and banner (pick, crop, GIF kept as is, remove), accent colour, about me (190). Not built, needs server fields: avatar decorations, profile effects, a two-colour profile theme, server tag display, markdown in about me.
- [x] **LNG-USET-010** [FE] Live preview card (popout style)
- [ ] **LNG-USET-011** [FE] Profile Layout entry ("MySpace mode", Phase 6 → EXT)
   - Open: deferred to the extension phase (Profile Layout); nothing to build now.

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
- [x] **LNG-USET-017** [FE] Activity privacy: share watching & reading (default on, friends + shared servers), per-section toggles, hide age-restricted from activity, pause history _(Activity page: master, watching, reading, age-restricted and pause switches stored in `user_prefs`; nothing publishes activity yet, so they take effect when Cinema and Library share it (p11-userset-fe))_
- [x] **LNG-USET-018** [FE] Display current activity as status, share detected activities _(Activity page: status and detected-activity switches, stored the same way (p11-userset-fe))_
- [ ] **LNG-USET-019** [FE] Request my data (status, download link)
   - The Request my data card states that the export builder (LNG-USET-020) does not exist; the button is disabled.
- [x] **LNG-USET-020** [BE][JOB] Data export package builder (JSON + attachment index, zipped, 7-day link) *(sr-db data_export.rs, sr-server userset_jobs.rs, routes /users/@me/data-export; tests data_export.rs, userset_pg_tests.rs)*

## Authorized Apps
- [x] **LNG-USET-021** [FE] OAuth2 grants list (app, scopes, Deauthorize)
   - Placeholder page only: the server has no OAuth2 grants to list or revoke, and the page says so.

## Devices
- [x] **LNG-USET-022** [FE] Current device + other sessions (platform, browser, location (coarse from IP, optional), last active), Log out one / all others (this device first, other sessions with Log out and Log out other devices, trusted devices; no coarse location (the server stores none))

## Connections
- [ ] **LNG-USET-023** [FE] Add connection buttons (providers grid), connected cards (display on profile toggle, show stats toggle, remove)
   - Page built with the Discord status and a read-only list; add/remove/refresh needs LNG-USET-024.
- [x] **LNG-USET-024** [BE] Connection OAuth flows per provider (Discord, GitHub, Spotify, Steam (OpenID), Twitch, YouTube, Reddit, Xbox/TikTok/Roblox where available) + unverified link connections + periodic stats refresh *(sr-api routes/userset connections.rs and providers.rs: GitHub, Discord, Spotify, Twitch, YouTube, Reddit, Steam behind [connections]; other providers link-only; GitHub/Reddit stats refresh job; tests userset/tests.rs)*
   - Discord is login/register only; `intent=link`, an unlink route and the other providers do not exist.

## SIDE ROOM: Invites
- [ ] **LNG-USET-025** [FE] Credits balance card ("You can bring N more people"), eligibility explanations
   - Open (backend): no route returns the account invite credit balance or eligibility (`GET /donations/me` has none). Credits can already be bought on the Support page.
- [ ] **LNG-USET-026** [FE] Create invite (note field) → code + copy + share link `/join/<code>`
   - Open (backend): no route creates, lists or revokes registration invites for the signed-in account (only `GET /sr/invites/{code}` and guild invites exist).
- [ ] **LNG-USET-027** [FE] Open codes list (note, created, Revoke), redeemed list (who joined, when)
   - Open (backend): same as LNG-USET-026 (open and redeemed codes lists).
- [x] **LNG-USET-028** [FE] Buy an invite ($20 BTC) → invoice modal _(Settings > Support / Patron "Invite credits": buy 1 to 5 for the configured price, then the invoice dialog (`support/SupportPanel.tsx`, test in `support/__tests__/support.test.tsx`))_
- [ ] **LNG-USET-029** [FE] Frozen/suspicion state notice (generic wording)
   - Open (backend): no frozen/suspicion state is exposed to the account.

## SIDE ROOM: Support & Perks
- [x] **LNG-USET-030** [FE] Patron status (active until, grace), Become a Patron / Extend (1/3/6/12 months) _(Patron card: active until, grace, permanent, 1/3/6/12 month buttons (`support/SupportPanel.tsx`))_
- [x] **LNG-USET-031** [FE] Perks table (free vs Patron) _(Free-versus-Patron perks table (`PerksTable`, `perkRows` in `support/SupportPanel.tsx`; p11-userset-fe))_
- [x] **LNG-USET-032** [FE] Invoices history (status, amount, txid link to configured explorer) _(Invoice history with state, amount and a "View transaction" link to the configured explorer (p11-userset-fe))_
- [x] **LNG-USET-033** [FE] Server contributions history _(Server contributions card listing paid server invoices with the server name (`Contributions`; p11-userset-fe). Reads `GET /donations/invoices`, no dedicated route)_
- [x] **LNG-USET-034** [FE] Leaderboard visibility (hide name, hide amount) _(Hide name / amount / badge switches on `PATCH /donations/me/preferences`, link to the leaderboards (`PrefsCard`))_

## SIDE ROOM: Activity & Libraries
- [ ] **LNG-USET-035** [FE] Watch history & reading history management (clear all, per item), defaults for Cinema (autoplay next, subtitle language, audio language, quality cap, data saver) and Library (default reading mode/direction, background, preload count)
   - Built (p11-userset-fe): Cinema watch history list with Clear all and the server-side pause, Cinema defaults (autoplay next, data saver, subtitle language), Library reader defaults (mode, direction, background). Open: per-item removal (no route), reading history list and clear (no route), audio language, quality cap and preload count (player/reader have no such setting).

## SIDE ROOM: Features
- [ ] **LNG-USET-036** [FE] User-scope feature modules via SchemaForm
   - Open (backend): only `/admin/features` exists; no user-scope feature module list or settings route.

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
- [x] **LNG-USET-046** [FE] Attenuation (lower others' volume while speaking) _(Attenuation slider: voices are lowered by 0 to 100 percent while you transmit (`voice/playback.ts` `setAttenuation`, `voice/controller.ts` `recompute`; p11-userset-fe))_
- [x] **LNG-USET-047** [FE] Camera select + preview, always preview video toggle _(Camera select (stored, used when the camera starts), live preview that releases the camera when stopped, "Always preview video" confirm dialog in the call controls (`lounge/voice/CameraPreview.tsx`; p11-userset-fe))_
- [x] **LNG-USET-048** [FE] Screen share defaults (resolution/fps within entitlement), share audio default _(Screen share defaults: quality preset, share-audio and content hint seed the Share dialog; entitlement stays enforced by the dialog (p11-userset-fe))_
- [ ] **LNG-USET-049** [FE] Sounds: join/leave, mute/deafen, PTT sounds, incoming ring, soundboard volume _(partial, feat/p5-voice-client: join/leave, mute/deafen and PTT sounds with a volume; incoming ring and soundboard volume wait for calls and the soundboard)_
   - Built: join/leave, mute/deafen, PTT and incoming call ring sounds with one volume (p11-userset-fe added the ring switch). Open: soundboard volume (no soundboard).
- [ ] **LNG-USET-050** [FE] Advanced: hardware acceleration hint, QoS, E2EE info + "show privacy codes"
   - Built: E2EE information note. Open: hardware acceleration hint and QoS (the engine has nothing to apply them to), "show privacy codes" (calls are not end-to-end encrypted yet).

## Chat
- [ ] **LNG-USET-051** [FE] Display images/videos/links previews, inline media size, show embeds, emoji reactions, convert emoticons, show stickers & autocomplete, spoiler display (on click / on hover for mine / always), markdown preview, show compose send button
   - Built: link previews, app embeds, inline attachments, inline media size (small/medium/large, p11-userset-fe), reactions, emoticon conversion, spoilers, remote images. Open: sticker autocomplete and markdown preview toggles (the composer has neither feature yet).

## Notifications
- [ ] **LNG-USET-052** [FE] Enable desktop notifications (permission request), unread badge, taskbar flash equivalent (title blink), push inactive timeout, sounds per type, reaction notifications, email notifications (digest toggles)
   - Built: default level; desktop permission request; browser notification for DMs and mentions while the window is in the background and a blinking tab title (`pwa/useDesktopAlerts.ts`, p11-userset-fe); unread badge; reaction, push timeout and email digest stored. Open: per-type sounds (message, mention) are stored but no message sound is played, reaction notifications have no producer, and the server delivers no Web Push or email digests.

## Keybinds
- [x] **LNG-USET-053** [FE] Keybind list (defaults read-only), add custom keybinds (PTT, toggle mute/deafen, navigate…), recorder (list with recorder for the Lounge actions; voice keybinds belong to the Voice & Video page)

## Language
- [x] **LNG-USET-054** [FE] Language picker (i18n framework), timestamp 12/24h format _(Interface language picker over the i18n framework (English only today, pseudo-languages in app test mode) plus the region and 12/24-hour clock; add a translation with `registerLocale` and it appears (p11-userset-fe))_

## Streamer Mode
- [x] **LNG-USET-055** [FE] Toggle + options (hide personal info, hide invite links, disable sounds/notifications) _(Own Streamer Mode section: master switch, hide personal info (email stays masked), hide invite links (invite dialog), disable sounds (voice cues and desktop alerts). Saved prefs, no automatic OBS detection (p11-userset-fe))_

## Advanced
- [x] **LNG-USET-056** [FE] Developer Mode (Copy ID items), app test mode for developers, reset UI state _(Developer mode flag (Copy ID items render only while Developer Mode is on, D-195 / G-56), App test mode (diagnostics card, pseudo-languages), Reset UI state (p11-userset-fe))_

## Removed sections (must not exist)
- [x] **LNG-USET-057** [TEST] Assert no Nitro, Server Boost (Discord), Subscriptions, Gift Inventory, Billing, Family Center, Clips, Quests sections are rendered

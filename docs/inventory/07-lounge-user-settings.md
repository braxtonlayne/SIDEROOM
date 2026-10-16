# 07 — Lounge: User Settings (`LNG-USET`)

Phase 1 (SIDE ROOM sections fill in as their phases land). Spec: [lounge-chat](../specs/lounge-chat.md) §12.

## Shell
- [x] **LNG-USET-001** [FE] Full-screen settings with search box, group headers (USER SETTINGS, SIDE ROOM, APP SETTINGS), What's New, Log Out (confirm), social links/version footer (search box over names and keywords, What's New dialog, version footer; no social links because the platform has none)
- [x] **LNG-USET-002** [BE] User settings blob (versioned JSON, partial PATCH, `USER_SETTINGS_UPDATE` dispatch across sessions). `GET/PATCH /users/@me/settings`; the dispatch is `SR_USER_SETTINGS_UPDATE` because `USER_SETTINGS_UPDATE` already carries presence (see also `/users/@me/notification-defaults` and `/users/@me/privacy`, LNG-USET-013/014 and LNG-NTF-001)

## My Account
- [x] **LNG-USET-003** [FE] Account card (banner, avatar, display name, username, email (reveal), Edit buttons per field) (banner, avatar, display name and username with an Edit each; email reveal)
- [x] **LNG-USET-004** [FE] Change display name / username (rate-limited) / email (password + verification) modals (display name, username (password re-auth, rate limit shown) and email dialogs)
- [x] **LNG-USET-005** [FE] Password & Authentication: Change Password modal, Enable Authenticator App wizard (QR, secret, verify code, backup codes download), View Backup Codes, Remove 2FA, Security Keys/Passkeys list (add, rename, remove) (Change Password on `POST /sr/auth/password` with optional sign-out of other devices; View Backup Codes is regenerate-only because codes are stored hashed)
- [x] **LNG-USET-006** [FE] Account Removal: Delete Account (explains permanence, D-029: messages remain, need a new invite to return), confirm with password + 2FA + typed username *(Settings > Account > Account removal, `settings/user/sections/DeleteAccount.tsx` over `POST /sr/auth/account/delete`: retention text, typed username, password, code with two-factor, `staff`, `owner` and `guilds` refusals; tests `account.test.tsx`)*
   - Card shown with the D-029 wording and a "not available yet" notice: the server has no account deletion (LNG-USET-007).
- [x] **LNG-USET-007** [BE] Account deletion process (state=deleted, profile scrub, remove relationships/memberships/sessions, revoke open invites, forfeit credits, release username after 30d) *(POST /sr/auth/account/delete; sr-db account_deletion.rs, migration 0660; tests account_deletion.rs and auth tests)*

## Profiles
- [x] **LNG-USET-008** [FE] Tabs: Main Profile / Per-server Profiles (server picker) (main profile and per-server tabs; the per-server profile is the nickname only)
- [x] **LNG-USET-009** [FE] Display name, pronouns, avatar (upload/crop/animated, remove), avatar decoration picker, profile effect picker, banner (image/animated or color), profile theme (two colors), about me (190 chars + markdown), server tag display picker *Built: settings/user/sections/Profile.tsx (avatar and banner upload/crop/animated/remove, display name, pronouns, about me 190, accent colour, profile effect and avatar decoration pickers over /sr/profile-catalog, two-colour profile theme `theme_colors`) and ServerTagCard.tsx; tests profile.test.tsx, p13profile.test.tsx.*
   - Built: display name, pronouns, avatar and banner (pick, crop, GIF kept as is, remove), accent colour, about me (190). Not built, needs server fields: avatar decorations, profile effects, a two-colour profile theme, server tag display, markdown in about me.
- [x] **LNG-USET-010** [FE] Live preview card (popout style)
- [ ] **LNG-USET-011** [FE] Profile Layout entry ("MySpace mode", Phase 6 → EXT) *Still open: Phase 6 / EXT, not started.*
   - Open: deferred to the extension phase (Profile Layout); nothing to build now.

## Content & Social
- [x] **LNG-USET-012** [FE] Sensitive media filters (DMs from friends / others / servers: show, blur, block) _p13 web: `settings/user/sections/SafetyCards.tsx` (SensitiveMediaCard, show/blur/block per audience over PATCH /users/@me/privacy), test `p13Safety.test.tsx`. The server stores the choice; applying it to attachments per viewer is still a backend gap (see the p13 settings-admin handoff)._
   - Needs a server: no media scanning or per-source filter setting exists. The page says so.
- [x] **LNG-USET-013** [FE] DM spam filter, message requests toggle, allow DMs from server members (default + per server) _p13 web: `SafetyCards.tsx` DmSafetyCard (spam filter off/non_friends/all and per-server DM overrides), message requests and server DMs toggles in `Privacy.tsx`; test `p13Safety.test.tsx`._
   - Built: message requests and allow DMs from server members on `/users/@me/privacy`. Not built, needs server fields: the DM spam filter and a per-server DM override.
- [x] **LNG-USET-014** [FE] Friend request sources (Everyone, Friends of Friends, Server Members) (Everyone / Friends of friends / Server members)
- [x] **LNG-USET-015** [FE] Age-restricted content (shows confirmation state, re-confirm flow) _p13 web: `SafetyCards.tsx` AgeConfirmationCard (state, confirm again, reset behind the password via DELETE /users/@me/age-confirmation; keeps the device copy in `lounge/AgeGate.tsx` in step); test `p13Safety.test.tsx`._
   - Needs a server: no way to read or reset the age confirmation (`POST /sr/age-gate` only records an answer). The page says so.
- [x] **LNG-USET-016** [FE] Blocked & ignored users list (unblock/unignore)

## Data & Privacy
- [x] **LNG-USET-017** [FE] Activity privacy: share watching & reading (default on, friends + shared servers), per-section toggles, hide age-restricted from activity, pause history _(Activity page: master, watching, reading, age-restricted and pause switches stored in `user_prefs`; nothing publishes activity yet, so they take effect when Cinema and Library share it (p11-userset-fe))_
- [x] **LNG-USET-018** [FE] Display current activity as status, share detected activities _(Activity page: status and detected-activity switches, stored the same way (p11-userset-fe))_
- [x] **LNG-USET-019** [FE] Request my data (status, download link) *Built: sections/DataExport.tsx over GET/POST /users/@me/data-export (status, polling while building, download link, size and expiry); tests p13connections.test.tsx.*
   - The Request my data card states that the export builder (LNG-USET-020) does not exist; the button is disabled.
- [x] **LNG-USET-020** [BE][JOB] Data export package builder (JSON + attachment index, zipped, 7-day link) *(sr-db data_export.rs, sr-server userset_jobs.rs, routes /users/@me/data-export; tests data_export.rs, userset_pg_tests.rs)*

## Authorized Apps
- [x] **LNG-USET-021** [FE] OAuth2 grants list (app, scopes, Deauthorize)
   - Placeholder page only: the server has no OAuth2 grants to list or revoke, and the page says so.

## Devices
- [x] **LNG-USET-022** [FE] Current device + other sessions (platform, browser, location (coarse from IP, optional), last active), Log out one / all others (this device first, other sessions with Log out and Log out other devices, trusted devices; no coarse location (the server stores none))

## Connections
- [x] **LNG-USET-023** [FE] Add connection buttons (providers grid), connected cards (display on profile toggle, show stats toggle, remove) *Built: sections/Connections.tsx over /users/@me/connections (providers grid with OAuth start and link-only forms, return notice for `?connected=`/`?error=`, show on profile, show stats, refresh, remove); tests p13connections.test.tsx.*
   - Page built with the Discord status and a read-only list; add/remove/refresh needs LNG-USET-024.
- [x] **LNG-USET-024** [BE] Connection OAuth flows per provider (Discord, GitHub, Spotify, Steam (OpenID), Twitch, YouTube, Reddit, Xbox/TikTok/Roblox where available) + unverified link connections + periodic stats refresh *(sr-api routes/userset connections.rs and providers.rs: GitHub, Discord, Spotify, Twitch, YouTube, Reddit, Steam behind [connections]; other providers link-only; GitHub/Reddit stats refresh job; tests userset/tests.rs)*
   - Discord is login/register only; `intent=link`, an unlink route and the other providers do not exist.

## SIDE ROOM: Invites
- [x] **LNG-USET-025** [FE] Credits balance card ("You can bring N more people"), eligibility explanations _(Settings > Invites: credits card from `GET /sr/invites`, with the reason a code cannot be made (`sections/Invites.tsx`, p12-admin-mod-be))_
- [x] **LNG-USET-026** [FE] Create invite (note field) → code + copy + share link `/join/<code>` _(create form with a private note on `POST /sr/invites`, copy code and `/join/<code>` link; tests `p12invites.test.tsx`, `sr-db/tests/member_invites.rs`)_
- [x] **LNG-USET-027** [FE] Open codes list (note, created, Revoke), redeemed list (who joined, when) _(open codes with Revoke (`DELETE /sr/invites/{code}`) and the redeemed list with who joined)_
- [x] **LNG-USET-028** [FE] Buy an invite ($20 BTC) → invoice modal _(Settings > Support / Patron "Invite credits": buy 1 to 5 for the configured price, then the invoice dialog (`support/SupportPanel.tsx`, test in `support/__tests__/support.test.tsx`))_
- [x] **LNG-USET-029** [FE] Frozen/suspicion state notice (generic wording) _(the suspicion freeze is shown as the generic `frozen` reason only)_

## SIDE ROOM: Support & Perks
- [x] **LNG-USET-030** [FE] Patron status (active until, grace), Become a Patron / Extend (1/3/6/12 months) _(Patron card: active until, grace, permanent, 1/3/6/12 month buttons (`support/SupportPanel.tsx`))_
- [x] **LNG-USET-031** [FE] Perks table (free vs Patron) _(Free-versus-Patron perks table (`PerksTable`, `perkRows` in `support/SupportPanel.tsx`; p11-userset-fe))_
- [x] **LNG-USET-032** [FE] Invoices history (status, amount, txid link to configured explorer) _(Invoice history with state, amount and a "View transaction" link to the configured explorer (p11-userset-fe))_
- [x] **LNG-USET-033** [FE] Server contributions history _(Server contributions card listing paid server invoices with the server name (`Contributions`; p11-userset-fe). Reads `GET /donations/invoices`, no dedicated route)_
- [x] **LNG-USET-034** [FE] Leaderboard visibility (hide name, hide amount) _(Hide name / amount / badge switches on `PATCH /donations/me/preferences`, link to the leaderboards (`PrefsCard`))_

## SIDE ROOM: Activity & Libraries
- [x] **LNG-USET-035** [FE] Watch history & reading history management (clear all, per item), defaults for Cinema (autoplay next, subtitle language, audio language, quality cap, data saver) and Library (default reading mode/direction, background, preload count) *Partial: watch and reading history lists with per-item Remove and Clear all, pause, Cinema (autoplay next, data saver, subtitle language) and Library reader defaults are built (sections/ActivityHistory.tsx, p13history.test.tsx). Missing: Cinema audio language and quality cap (the player cannot apply them before playback) and a Library preload count.* *(P15 rest-b: the player can apply both after all: the preferred audio language is picked when the track list arrives (`cinema/player/session.ts` `pickDefaultAudio`), the quality cap is the lower of cap and data saver (`maxHeight`), both stored in `cinema/player/defaults.ts`; Library `preloadCount` (1..10, `library/prefs.ts`, `Reader.tsx`); cards in `sections/ActivityHistory.tsx`; tests `session.test.ts` "settings defaults", `settings/user/__tests__/p15Defaults.test.tsx`)*
   - Built (p11-userset-fe): Cinema watch history list with Clear all and the server-side pause, Cinema defaults (autoplay next, data saver, subtitle language), Library reader defaults (mode, direction, background). Open: per-item removal (no route), reading history list and clear (no route), audio language, quality cap and preload count (player/reader have no such setting).

## SIDE ROOM: Features
- [ ] **LNG-USET-036** [FE] User-scope feature modules via SchemaForm *Still open: needs a user-scope module schema API; not started.*
   - Open (backend): only `/admin/features` exists; no user-scope feature module list or settings route.

## Appearance
- [x] **LNG-USET-037** [FE] Theme: Light / Dark / Ash / Onyx / Sync with computer; Side Room theme option (Light / Dark / Ash / Onyx / Sync with computer / Side Room)
- [x] **LNG-USET-038** [FE] Per-section themes (Lounge / Cinema / Library / Entrance) pickers (Lounge, Cinema, Library and Entrance pickers (Cinema applies when its shell reads the preference))
- [ ] **LNG-USET-039** [FE] Theme Store entry (approved packages) *Still open: no approved-packages Theme Store endpoint for the web client yet.*
   - Theme Store entry not built (there is no theme package store yet).
- [x] **LNG-USET-040** [FE] Message display (Cozy/Compact), show avatars in compact, chat font scaling, space between message groups, zoom level *Partial: Cozy/Compact, chat font scale, message gap and zoom are built (sections/Appearance.tsx). p14: "Show avatars in Compact mode" (`compactAvatars` pref, MessageItem.tsx, msgGaps.test.tsx).* 
   - Built: cozy/compact, chat font scale, space between groups, zoom. Not offered: show avatars in compact (compact has no avatar column).
- [x] **LNG-USET-041** [FE] Allow server theming toggle, show section top bar toggle (allow server theming; the section top bar toggle stays device-local in the Lounge store)

## Accessibility
- [x] **LNG-USET-042** [FE] Saturation, link decorations, role colors display (name / dot / off), reduced motion (sync), autoplay GIFs/stickers/emoji, contrast, TTS rate, show send-message button, dyslexia-friendly font option *Built: sections/Accessibility.tsx (saturation in Appearance, underline links, role colours name/dot/off, reduced motion, sticker and emoji animation, high contrast, text-to-speech speed via `ttsRate`, send button, dyslexia font).*
   - Built: saturation, link underline, role colours (name/dot/off), reduced motion, sticker and emoji animation, high contrast, dyslexia-friendly text, send button. Not built: TTS rate (no TTS reader), autoplay of GIFs (no GIF autoplay control in the client).

## Voice & Video
- [x] **LNG-USET-043** [FE] Input/output device selects, input/output volume sliders, mic test with level meter
   - Voice & Video page: owned by the voice client work, not touched here.
- [x] **LNG-USET-044** [FE] Input mode: Voice Activity (auto sensitivity toggle / manual threshold slider with live meter) or Push to Talk (keybind recorder, release delay slider)
- [x] **LNG-USET-045** [FE] Echo cancellation, auto gain toggles (noise suppression intentionally absent, D-045)
- [x] **LNG-USET-046** [FE] Attenuation (lower others' volume while speaking) _(Attenuation slider: voices are lowered by 0 to 100 percent while you transmit (`voice/playback.ts` `setAttenuation`, `voice/controller.ts` `recompute`; p11-userset-fe))_
- [x] **LNG-USET-047** [FE] Camera select + preview, always preview video toggle _(Camera select (stored, used when the camera starts), live preview that releases the camera when stopped, "Always preview video" confirm dialog in the call controls (`lounge/voice/CameraPreview.tsx`; p11-userset-fe))_
- [x] **LNG-USET-048** [FE] Screen share defaults (resolution/fps within entitlement), share audio default _(Screen share defaults: quality preset, share-audio and content hint seed the Share dialog; entitlement stays enforced by the dialog (p11-userset-fe))_
- [x] **LNG-USET-049** [FE] Sounds: join/leave, mute/deafen, PTT sounds, incoming ring, soundboard volume _(partial, feat/p5-voice-client: join/leave, mute/deafen and PTT sounds with a volume; incoming ring and soundboard volume wait for calls and the soundboard)_ *Unchanged: incoming ring and soundboard volume wait for calls and the soundboard.* *(P15 rest-b: incoming ring is the `ring` cue + `soundRing` switch (`voice/sounds.ts`, `VoiceVideo.tsx`); the soundboard volume slider is now also in the Sounds card, same store as the popover; test `settings/user/__tests__/p15Advanced.test.tsx`)*
   - Built: join/leave, mute/deafen, PTT and incoming call ring sounds with one volume (p11-userset-fe added the ring switch). Open: soundboard volume (no soundboard).
- [x] **LNG-USET-050** [FE] Advanced: hardware acceleration hint, QoS, E2EE info + "show privacy codes" *Still open: only Developer Mode, App Test Mode and diagnostics exist; hardware acceleration hint, QoS and E2EE info are not built.* *(P15 rest-b: `sections/AdvancedVoice.tsx`: hardware acceleration hint (WebGL renderer check), QoS "High packet priority" (`voice/settings.ts` `qosHighPriority` -> `ConnectParams.highPriority` -> mic sender `priority`/`networkPriority`), E2EE info and "Show privacy codes" (`showPrivacyCodes`, honoured by `lounge/voice/PrivacyCodes.tsx`); tests `p15Advanced.test.tsx`, `voice/__tests__/p15Qos.test.ts`)*
   - Built: E2EE information note. Open: hardware acceleration hint and QoS (the engine has nothing to apply them to), "show privacy codes" (calls are not end-to-end encrypted yet).

## Chat
- [x] **LNG-USET-051** [FE] Display images/videos/links previews, inline media size, show embeds, emoji reactions, convert emoticons, show stickers & autocomplete, spoiler display (on click / on hover for mine / always), markdown preview, show compose send button *Partial: link previews, embeds, inline media size, reactions, emoticon conversion and spoiler display (click / always) are built (sections/TextImages.tsx). Missing: sticker autocomplete switch, "on hover for mine" spoilers and a markdown preview.* *(P15 rest-b: all three built: spoiler mode "on hover for mine" (`sections/TextImages.tsx`, `prefs-effects.css`, `MessageItem.tsx`), sticker suggestions switch (`lounge/msg/ComposerExtras.tsx` `StickerSuggestions`) and Markdown preview (`MarkdownPreview`); tests `lounge/__tests__/p15ComposerExtras.test.tsx`)*
   - Built: link previews, app embeds, inline attachments, inline media size (small/medium/large, p11-userset-fe), reactions, emoticon conversion, spoilers, remote images. Open: sticker autocomplete and markdown preview toggles (the composer has neither feature yet).

## Notifications
- [x] **LNG-USET-052** [FE] Enable desktop notifications (permission request), unread badge, taskbar flash equivalent (title blink), push inactive timeout, sounds per type, reaction notifications, email notifications (digest toggles) *Built: sections/Notifications.tsx (desktop notification permission request, unread badge, title blink, push inactive timeout, sounds per type, reactions, email digest, DM pop-ups `dmToasts`).*
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

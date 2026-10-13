# Handoff: feat/p5-user-settings

Branch `feat/p5-user-settings` (base `integration/phase4`), worktree `SIDEROOM_wt/p5-user-settings`. Local commits only, nothing pushed. Frontend only: no Rust, no migration, no OpenAPI change (every endpoint needed already existed).

## Done

| Area | What changed | Where |
|---|---|---|
| Preferences model | One validated `UserPrefs` object (about 45 fields). The browser copy stays the first paint; the account copy is `user_prefs` in the settings blob (`GET/PATCH /users/@me/settings`). The account wins when it loads, another device's change arrives over `SR_USER_SETTINGS_UPDATE`, a browser with local choices and an empty account pushes them up once, removed entries in the two record fields go out as explicit `null`s. `useAppliedTheme` moved to its own file and starts the sync, so Lounge and Library pick it up. | `settings/user/{prefs,prefsSync,useAppliedTheme}.ts` |
| Shell | Search box over setting names and keywords (opens the section, scrolls to the card, Escape clears first), What's New dialog, version footer, 7 new sections (Activity Privacy, Authorized Apps, Devices, Connections, Accessibility, Text & Images; Profile renamed Profiles). `SettingsLayout` got an optional `sidebarHeader` (additive). | `settings/user/{nav,searchIndex,SettingsSearch,whatsNew,UserSettings}`, `packages/ui` |
| My Account | Avatar and banner on the card, Edit for username (`PUT /sr/auth/username`, password, 2/day) and display name, Change Password wired to `POST /sr/auth/password` (optional sign-out of other devices), Account Removal card (states the gap). | `sections/{Account,AccountEdit}.tsx` |
| Profiles | Display name, avatar and banner upload with a crop dialog (drag, arrow keys, zoom; GIFs kept as they are; remove), per-server tab with the server nickname (`PATCH /guilds/{id}/members/{me}`). Pictures wait for Save changes like the other fields. | `sections/Profile.tsx`, `ImageCropDialog.tsx`, `imageCrop.ts` |
| Privacy & Safety | Friend-request sources, server-member DMs and Message Requests on `/users/@me/privacy` (saved on toggle, reverted on error), blocked/ignored (kept), remote images (kept), notices for the media filters, age confirmation and data request. | `sections/Privacy.tsx` |
| Devices / Connections | Sessions moved here (this device first, Log out, Log out other devices, Log out everywhere) with trusted devices; Discord status moved to Connections. | `sections/{Devices,Connections}.tsx` |
| Appearance | Theme incl. Side Room, per-section themes (Lounge, Cinema, Library, Entrance; applied through `ThemeProvider perSection`, the Entrance reads the browser copy), server theming, section bar toggle, message display, space between groups, font scale, zoom, saturation. | `sections/Appearance.tsx`, shells |
| Accessibility | Reduced motion (moved here), high contrast, always underline links, role colours (name/dot/off), dyslexia-friendly text, sticker and emoji animation (first-frame freeze on stickers), send button. | `sections/Accessibility.tsx`, `prefs-effects.css`, `lounge/msg/{Sticker,staticFrame}` |
| Text & Images | Link previews, app embeds, inline attachments, reactions, emoticon conversion at send, spoilers, remote images. | `sections/TextImages.tsx`, `lounge/{MessageItem,Composer,emoticons}`, `lounge/msg/Attachments.tsx` |
| Notifications | Default level on `/users/@me/notification-defaults` (replaces the browser-only radio), per-server overrides (kept), unread badge, title blink, reaction notifications, push inactivity, email digest, desktop and sounds (all in `user_prefs`). | `sections/Notifications.tsx` |
| Keybinds | Defaults plus a recorder for the Lounge actions (conflict check, modifier rule, Escape cancels, unbind, reset one/all). The Lounge handler and the shortcut sheet use the effective keys. | `sections/Keybinds.tsx`, `lounge/shortcuts.ts` |
| Language | Region list and 12/24-hour clock, applied to message times and dates. | `sections/App.tsx`, `lounge/time.ts` |
| Advanced | Developer mode flag, Reset UI state (clears the Lounge layout and its account copy). | `sections/App.tsx` |
| Activity Privacy | All switches, stored in `user_prefs`. | `sections/Activity.tsx` |

## Not done / gaps (the page states each one)

- Connections: Discord is login/register only. No `intent=link`, no unlink route, no other providers, no stats refresh (LNG-USET-023/024).
- Authorized Apps: no OAuth2 grants on the server, so the page is a placeholder (LNG-USET-021).
- Account deletion (LNG-USET-006/007) and the data export builder (LNG-USET-019/020) do not exist server-side.
- Sensitive-media filters, DM spam filter, per-server DM override, reading or resetting the age confirmation: no server fields (LNG-USET-012/013/015).
- Profile: avatar decorations, profile effects, two-colour theme, server tag, per-server avatar/banner/bio need new profile fields (LNG-USET-009).
- Activity Privacy: nothing publishes activity yet, so the switches are stored and not read.
- Notifications: the client raises no desktop notifications and the server sends no Web Push or digest email, so those choices are stored only. `unreadBadge` and `titleBlink` have no consumer yet.
- Language: no translation framework on the branch (p5-xc), so only dates and times follow the choice; strings are English constants. When the hook lands, `prefs.locale` is the value to feed it.
- Not offered: TTS rate, GIF autoplay control, show avatars in compact, inline media size, markdown preview toggle, Streamer Mode, Theme Store entry, app test mode, Invites/Support/Features sections (not in this task).
- Developer mode is saved but gates nothing (Copy ID items were already always shown).
- Voice & Video page and voice keybinds: untouched. The voice agent adds its section to `USER_SETTINGS_SECTIONS` (one line per section) and its entries to `searchIndex.ts`. Voice keybinds can live in their own prefs field; `prefs.keybinds` only holds Lounge shortcut ids, and `sanitizePrefs` drops anything that is not a valid combo.

## Tests and gates

- web: 690 tests in 56 files passing (new: account +7, profile +7, privacy +3, notifications +3, a 25-test file for the new sections, search, sync and keybinds, 12 logic tests, 10 Lounge effect and time tests). ui: 196 passing (+1 for the sidebar header). api-client, markdown-react and wasm are untouched.
- Gates run: `pnpm -r typecheck`, `pnpm -r lint`, `prettier --check apps packages e2e docs`, `vitest run` (web, ui), `vite build`. Axe checks are in the new account, privacy, accessibility, text, activity, apps, connections, language, crop dialog and Lounge-effects tests.
- Not run: Playwright e2e (ports 8080/5173 are shared; the passkeys spec only uses `/settings/user/account`, which still hosts the passkeys card), Rust gates (no Rust touched).

## Decisions

1. Preferences are one object under one blob key rather than many top-level keys: one merge patch per save, one place to validate. `load_remote_images` stays separate because it already shipped and saves on toggle.
2. Server-backed settings (default notification level, privacy rules, nickname, remote images, reset layout) save immediately; everything in `user_prefs` and the profile goes through the existing unsaved-changes bar.
3. Dyslexia-friendly text uses a system font stack and wider spacing; no font is downloaded (OpenDyslexic would be an asset and a licence decision).
4. A rebound shortcut loses its default (the old key no longer works), Escape and "any key" stay fixed, and a bare letter is refused because it would be typed into the message box.
5. "Show the send button" defaults to on because the button was always shown and tests rely on it; Discord defaults it off on desktop.
6. Cropped pictures are drawn at 512 px (avatar) and 1200 x 480 (banner), PNG unless the source was JPEG; the server crops and caps again.
7. Sticker/emoji "never" and "on hover" freeze the first frame through a canvas; if the browser refuses to read the image (cross-origin without CORS) the animated image is shown. There is no custom-emoji URL resolver in the web app yet, so the emoji setting is stored and takes effect once there is one.

## Resume

```
cd C:/Users/braxt/Desktop/SIDEROOM_wt/p5-user-settings
export npm_config_manage_package_manager_versions=false
npx pnpm@10.12.1 --filter @sideroom/wasm build
cd apps/web && npx tsc --noEmit && npx eslint . && npx vitest run --maxWorkers=3
```

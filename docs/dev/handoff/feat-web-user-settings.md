# Handoff: feat/web-user-settings (COMPLETE)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/usettings, branch feat/web-user-settings (base integration/phase1). Local commits only.

## Status
All sections built and tested. Gates (run separately): typecheck, lint, test (api-client 4, wasm 2, markdown-react 37, ui 186, web 232), `--filter @sideroom/web build`, `npx prettier --check apps packages` all pass. Not run: e2e (merge step).

## Sections
- My Account: done (identity, email reveal, resend verification, TOTP wizard with QR + recovery codes, regenerate, disable, Discord status read-only, trusted devices, sessions, log out everywhere). Change password STUBBED (disabled, plus working emailed-reset-link).
- Profile: done (bio, pronouns, accent, custom status, preview, counter). Display name read-only; avatar/banner disabled "coming soon".
- Privacy and Safety: blocked + ignored lists done. DM / friend-request privacy STUBBED (disabled).
- Appearance: done, local only (localStorage key sr.user.prefs), applied via ThemeProvider and the Lounge shell. Compact = tighter spacing only.
- Notifications: per-server overrides wired; default level, sounds, desktop toggle local only.
- Keybinds read-only, Language (English, disabled), Advanced (developer mode flag, local), Log Out with confirm: done.
- Entry points: Lounge gear, Ctrl+, ; routes /settings/user/:section; ESC returns to the opening location (fallback /channels). No quick-switcher settings entry exists in the codebase.

## Missing backend endpoints
- Change password while signed in (only POST /sr/auth/forgot and /sr/auth/reset exist).
- Change display name / username / email (PATCH /users/@me accepts only bio, pronouns, banner, accent_color, custom_status).
- User settings blob, GET/PATCH /users/@me/settings (LNG-USET-002), so appearance/notification defaults are not synced.
- Global default notification level (only /users/@me/guilds/{id|@me}/settings).
- DM and friend-request privacy settings (no route; logic exists in sr-chat only).
- Link/unlink Discord for a signed-in user (GET /sr/auth/discord/start accepts intent=login|register only; no unlink).
- Avatar/banner upload (other agent).

## Inventory ticked
LNG-USET-010, 016, 057. Others partial, left unticked.

## Notes
- vite.config.ts testTimeout raised to 30000 (jsdom suites are slow).
- Prettier was also applied to 7 pre-existing apps/web files that failed the check (formatting only).

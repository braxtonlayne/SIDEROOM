# Handoff: feat/web-user-settings

Worktree: C:/Users/braxt/Desktop/SIDEROOM_wt/usettings, branch feat/web-user-settings (base integration/phase1). Not pushed.

## Goal and scope
Full-screen User Settings at apps/web/src/settings/user/ (route /settings/user/:section), plus generic UI primitives in packages/ui. Only apps/web/** and packages/ui/** touched.

## DONE
- packages/ui: QRCode (inline SVG) + util/qr.ts encoder (byte mode, L/M, v1-10), KeybindRow, SettingsLayout roving tabindex + sidebarFooter. Tests: packages/ui/src/primitives/settings-extras.test.tsx (7 pass, includes RS vector and matrix round-trip). misc.test.tsx still passes.
- apps/web: all sections written and typechecking: account (identity, email reveal, resend verification, password card disabled + emailed reset link, TOTP wizard with QR/recovery codes, regenerate, disable, Discord status read-only, trusted devices, sessions, log out everywhere), profile (PATCH /users/@me bio/pronouns/accent/custom status, live preview, counter), privacy (blocked/ignored lists; DM/friend privacy disabled), appearance (local prefs, live preview), notifications (default level local, desktop permission, sounds, per-server overrides via /users/@me/guilds/{id}/settings), keybinds, language, advanced, Log Out confirm.
- Entry points: user panel gear, Ctrl+, shortcut (shortcuts.ts, LoungeShell), router.tsx registers routes, Lounge ThemeProvider now applies saved prefs; compact CSS in lounge.css.
- Existing web suite (173 tests) passed after the integration, before the new tests were added.

## IN PROGRESS
- apps/web/src/settings/user/__tests__/helpers.tsx and account.test.tsx written (uncommitted until this commit). 14 of 16 pass. Failing:
  1. "enrolls TOTP..." : navigator.clipboard.writeText spy not called (clipboard defineProperty probably overridden by userEvent.setup(); stub after setup or use vi.spyOn on userEvent's clipboard).
  2. "lists trusted devices..." : getByText('Laptop') matches both the strong and the sr-only span; use getAllByText or query the list item.
  Same sr-only duplication may affect other getByText calls.

## REMAINS
- Fix the two tests above; write tests: profile, appearance, notifications, privacy, shell (nav roving, ESC returns to previous location from Lounge gear, Ctrl+, opens, unsaved bar blocks ESC / Save / Reset, logout confirm, keybinds/language/advanced), axe per section.
- Run separately: typecheck, lint, test, `--filter @sideroom/web build`, `npx prettier --check apps packages` (7 pre-existing prettier failures exist in base: inviteFlow.ts, ServerInvite.tsx, rest.ts, store.ts etc.; do not reformat them).
- Tick LNG-USET inventory items only for wired work (likely 003 partial, 005 partial, 008/009/010 partial, 016, 022, 040 partial, 052 partial, 053 read-only, 056 partial); none ticked yet.
- Quick switcher has no settings entry in this codebase (not added).

## Missing backend endpoints
- Change password while signed in (no route; only /sr/auth/forgot + /sr/auth/reset).
- Edit display name/global_name, username, email (PATCH /users/@me only takes bio, pronouns, banner, accent_color, custom_status).
- User settings blob (LNG-USET-002), so appearance/notification defaults are localStorage only.
- Global default notification level (only per-guild/@me scope exists).
- DM / friend-request privacy settings (no route).
- Link/unlink Discord from settings (discord/start only accepts intent=login|register).
- Avatar/banner upload (other agent).

## Decisions / notes
- Default theme kept lounge-dark; "Sync with computer" opt-in.
- Compact message display only tightens spacing.
- Per-server notification overrides save immediately, not through the unsaved bar.

## Resume
cd C:/Users/braxt/Desktop/SIDEROOM_wt/usettings; `npm_config_manage_package_manager_versions=false npx pnpm@10.12.1 --filter @sideroom/wasm build` (already built once), then `... exec vitest run src/settings` in apps/web.

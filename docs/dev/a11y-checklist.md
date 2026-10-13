# Accessibility: automated coverage and the manual pass (XC-A11Y)

## What runs automatically

| Check | Where | Covers |
|---|---|---|
| axe on every primitive and composed screen | Vitest, `packages/ui` and `apps/web` (jsdom) | roles, names, ARIA validity, labels. Not contrast or landmarks (jsdom has no layout) |
| axe in a real browser | `e2e/static/a11y.spec.ts` (`pnpm --filter @sideroom/e2e test:static`, CI job `static-browser-checks`) | WCAG 2 A/AA, 2.1 A/AA and best-practice rules, **including colour contrast and landmarks**, on every public entrance route |
| Skip link, `main` landmark, one `h1` | same spec | first Tab stop moves focus to `main` |
| Reduced motion, forced colours | same spec | animations collapse, focus outline survives Windows high contrast |
| Focus trap, Escape, focus restore | `packages/ui/src/primitives/overlays.test.tsx`, `menus.test.tsx` | Modal, Drawer, Menu, Popover |

Signed-in screens (Lounge, Library, Admin, Settings) are not in the browser axe run yet because they need a session; add a route to `ROUTES` once a fixture user exists (XC-TEST-004), or stub `/api/v1/users/@me` in the spec.

## Conventions

- Every page has one `<main>`; `SkipLink` (in `EntranceShell`; mount it in the other shells) targets it.
- Announce changes with `announce(text)` from `apps/web/src/a11y/announcer.tsx` (polite for new messages and saves, assertive for dropped calls). One pair of live regions exists for the whole app (`<LiveRegions />` in `main.tsx`).
- Never rely on colour alone. Muted text must reach 4.5:1: entrance copy uses `--ent-muted` because `--sr-text-3` is about 3.6:1 (see `components/entrance.css`).
- New icon-only buttons get an `aria-label` from the i18n catalog, never a hard-coded string.
- Respect `prefers-reduced-motion`; the global rule in `styles/a11y.css` is a safety net, not a licence to animate.

## Manual screen-reader pass (before each phase exit)

Do each flow with the keyboard only first, then with the reader. Note the browser and reader pair in the PR.

| Pair | Why |
|---|---|
| NVDA + Firefox, NVDA + Chrome (Windows) | Most common free reader |
| VoiceOver + Safari (macOS), VoiceOver + Safari (iOS) | Required for the installed PWA on iOS |
| TalkBack + Chrome (Android) | Installed PWA on Android |

Flows to walk:

1. Landing: skip link, invite field label and status message ("You're on the list"), the error when a code is refused.
2. Sign in, including the second factor step and the passkey button.
3. Lounge: move between servers, channels and messages with the keyboard; a new message in the open channel is announced politely and does not steal focus.
4. Open and close a modal, a context menu and the quick switcher: focus lands inside, Escape closes, focus returns to the opener.
5. Send a message with an attachment; the upload progress and completion are announced.
6. Join a voice channel: join, mute, leave and a dropped connection are announced.
7. Library reader: page turn, chapter change and the settings drawer.
8. Settings: change a value, see "Settings saved" announced, close with Escape.

Record any failure as an issue with the reader, browser, steps and what was heard.

# Accessibility: automated coverage and the manual pass (XC-A11Y)

## What runs automatically

| Check | Where | Covers |
|---|---|---|
| axe on every primitive and composed screen | Vitest, `packages/ui` and `apps/web` (jsdom) | roles, names, ARIA validity, labels. Not contrast or landmarks (jsdom has no layout) |
| axe in a real browser | `e2e/static/a11y.spec.ts` (`pnpm --filter @sideroom/e2e test:static`, CI job `static-browser-checks`) | WCAG 2 A/AA, 2.1 A/AA and best-practice rules, **including colour contrast and landmarks**, on every public entrance route |
| Skip link, `main` landmark, one `h1` | same spec | first Tab stop moves focus to `main` |
| Reduced motion, forced colours | same spec | animations collapse, focus outline survives Windows high contrast |
| Focus trap, Escape, focus restore | `packages/ui/src/primitives/overlays.test.tsx`, `menus.test.tsx` | Modal, Drawer, Menu, Popover |

Signed-in screens are covered twice. In jsdom by `apps/web/src/a11y/signedInRoutes.test.tsx` (a table of routes with the API mocked by MSW; add a row per new route) and by the axe cases in the Library, Cinema, Admin and Developers suites. In a real browser by `e2e/tests/a11y-signed-in.spec.ts` (RDY-A11Y-001): axe, including colour contrast and landmarks, over a fixture world built through the API (`e2e/support/world.ts`: a server with messages and a thread, a published Cinema title, a watch party) and every route listed in the spec: Lounge channel, thread and home, 19 Settings sections, server settings, Library home and browse, Cinema home, movies, title, player, party, Live TV guide and My list, Admin home, Developers, Developers docs and Support. Serious and critical findings fail the test; the rest are written to `e2e/.state/a11y/<route>.json` and listed in the handoff. The `playwright` CI job runs it on Chromium, Firefox and WebKit.

| Check | Where | Covers |
|---|---|---|
| Keyboard flows 1 to 8 of the manual pass below | `e2e/tests/keyboard-<n>-*.spec.ts` | keyboard-only (Tab, arrows, Enter, Space, Escape), focus position and live-region text at each step; flow 6 (voice) runs in the `live` job, flow 5 (attachment) needs object storage |
| Reduced motion and High contrast in a browser | `e2e/tests/a11y-modes.spec.ts` | the OS preference and the in-app setting collapse every animation in the Lounge, Cinema and Library; the in-app High contrast setting lifts muted text and leaves no `color-contrast` violation |
| The token set | `packages/ui/src/theme/tokens-contrast.test.ts` | every theme: text on surfaces 4.5:1, ink on fills, focus and status 3:1; High contrast resolves every text token to full strength (7:1) and borders to 3:1 |
| The CSS guard | `apps/web/src/a11y/reducedMotion.test.ts` | the OS and in-app reduced-motion guards in `styles/a11y.css`, no `!important` motion that defeats them, no smooth scroll without a check |

A WebKit build that does not Tab to links (the Safari default; Playwright's Windows WebKit is one) cannot walk the flows that cross a link: those specs probe for it and skip with a reason. On Linux (CI) Tab reaches links.

## Conventions

- Every page has one `<main>`; `SkipLink` (in `EntranceShell`; mount it in the other shells) targets it.
- Announce changes with `announce(text)` from `apps/web/src/a11y/announcer.tsx` (polite for new messages and saves, assertive for dropped calls). One pair of live regions exists for the whole app (`<LiveRegions />` in `main.tsx`).
- Never rely on colour alone. Muted text must reach 4.5:1: entrance copy uses `--sr-entrance-text-muted` because `--sr-text-3` is about 3.6:1 (see `components/entrance.css`).
- New icon-only buttons get an `aria-label` from the i18n catalog, never a hard-coded string.
- Respect `prefers-reduced-motion`; the global rule in `styles/a11y.css` is a safety net, not a licence to animate.

## Keyboard navigation map (XC-A11Y-001)

Tab moves between regions; arrow keys move inside a list. A list is one Tab stop (roving tabindex, `a11y/useRovingTabindex.ts`, used through `<RovingNav>`). Home and End jump to the ends. Shortcuts below are the defaults and can be rebound in Settings, Keybinds; the full list is in the shortcut sheet.

| Section | Region | Keys |
|---|---|---|
| Everywhere | Skip link (entrance pages, Lounge, Cinema, Library, Admin; Developers has its own) | Tab once, Enter: focus moves to `main` |
| Everywhere | Dialogs, menus, popovers | Focus trapped and restored; Escape closes; arrows move in menus |
| Lounge | Server rail | Roving: Up/Down move, Home/End ends, Enter opens. Ctrl+Alt+Up/Down switch server (Cmd on macOS) |
| Lounge | Channel list | Roving across channels and category headers: Up/Down, Home/End, Enter/Space (category toggles). Alt+Up/Down previous or next channel, Alt+Shift+Up/Down next unread |
| Lounge | Message list | One Tab stop (roving tabindex): Up/Down/Home/End move between messages, each message's own buttons stay tabbable. The list is a log region: new messages are announced and focus never moves. Esc marks the channel read; typing a character focuses the composer. |
| Lounge | Composer | Enter sends, Shift+Enter newline, autocomplete popup: Up/Down/Tab/Enter/Esc |
| Lounge | Member list | One Tab stop (roving tabindex): Up/Down/Home/End move between the members that are mounted; focus scrolls the next ones in. |
| Lounge | Quick switcher | Ctrl+K opens (Cmd on macOS); Up/Down, Enter, Esc. Ctrl+/ opens the shortcut sheet, Ctrl+, user settings |
| Library | Search results, series grid, reader | Card grids and poster rows are one Tab stop: arrows move by card, Up/Down by row in a grid (`useRovingTabindex` with `orientation: 'grid'`), Home/End jump. Reader: arrows turn pages, Esc leaves. |
| Cinema | Browse rows, live guide grid | Poster rows and the title grid are one Tab stop with arrows (as the Library). The live guide grid has its own arrow-key grid navigation (`cinema/tv/GuideGrid.tsx`, one Tab stop, tested in `cinema/tv/__tests__/guide.test.tsx`). |
| Settings and Admin | Section nav, forms | Tab through nav then content; section nav uses the `SettingsLayout` roving list; Esc closes settings and focus returns to the control that opened them |
| Share | Channel picker | Tab per channel, Enter picks |

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

### Which pairs have been heard

None yet. The keyboard half of every flow is automated (RDY-A11Y-003), and the live regions are asserted as text, but nobody has listened to them with a reader. All of these remain unheard and are a human action (docs/dev/roadmap-p20.md, "Needs the owner"):

| Pair | Heard |
|---|---|
| NVDA + Firefox | no |
| NVDA + Chrome | no |
| VoiceOver + Safari (macOS) | no |
| VoiceOver + Safari (iOS) | no |
| TalkBack + Chrome (Android) | no |

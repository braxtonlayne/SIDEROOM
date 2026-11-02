# fu-ui (XC-VIS-004): link token, visual leftovers, /u/<member>

Branch `fix/fu-ui`.

## Done
- **S6** `--text-link` registered in sr-theme (`registry.rs`, regenerated list in `tokens.generated.ts`), values moved into
  `packages/ui/src/tokens/themes.ts` (Dark/Onyx #00a8fc, Ash #1ab2ff, Light #4752c4 = the deeper accent, Editorial and Side
  Room gold). `app-links.css` and its import are gone. `textLink.test.ts` now reads the theme definitions (every theme has a
  value, AA on three surfaces, file is gone). Rust tests: registry entry, custom theme may set it (and a bad value is rejected).
- Hover toolbar clipped on the message at the list top: `MessageItem` sets `data-tools-inside` when the message is within 18px
  of the scroller top; toolbar then sits inside it (measured: was 16px above the scroller, now 2px inside).
- "Message Requests" cut off at 240px: nav item gap/padding 12 -> 10 (label now fits, scrollWidth == clientWidth).
- profile-blocks avatar initial: ink from the accent (`--pp-accent-ink`, white/near-black by WCAG contrast), tested.
- TV guide last label cut ("6:"): labels the scroller edge would cut are hidden (IntersectionObserver, `data-clipped`).
- Settings > Modules save bar (phone): buttons share one row (Save keeps one line, Discard wraps).
- Features group labels: `.ss-group` spacing (20px above label, 8px below).
- `/u/<member>` header shows the display name once the page loads (`lounge.profilePage.titleNamed`, `fu-ui-1.ts`).

## Not done
- "This profile is private": the API (`PackageService::page`) deliberately returns the same 404 for missing, blocked and
  hidden pages (no oracle), so the client cannot tell them apart. "No such profile" stays; its body already says "or you cannot
  see their page". The header shows the username in that case.
- No unit tests for the toolbar/TV label/save-bar CSS (verified by measurement in the browser instead).

## Shots (C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots)
`fu-ui-before`, `fu-ui-after`: lounge-home, lounge-channel, cinema-tv, settings-user-modules, settings-server-features,
profile-u, profile-member; desktop and phone, light and dark. `fu-ui-check`: hover toolbar, modules bar, customised profile.
Note: the worktree's `packages/wasm/dist` lacked music and tools; I built those two (dev profile) to run the app.

## Tests run
web tsc, eslint (changed files), prettier, vitest (lounge, cinema/tv, settings, admin, profilepage, styles: 1726 pass),
ui theme tests, profile-blocks tests, check-css-tokens, i18n-audit, cargo fmt/clippy/test -p sr-theme, web build,
check-entrance-isolation, check-bundle-budget, `playwright test --list` (888 tests load).

## Owner questions
1. Should the API tell a hidden profile from a missing one to a signed-in viewer ("This profile is private")? Default: no (no oracle).
2. A server accent recolour no longer tints Lounge Light links (the value is a fixed hex, AA-safe). OK?
3. The toolbar overlaps the first line of the top message's text instead of hanging above it. OK?
4. Long TV slot labels at the right edge disappear until they fit, instead of being cut. OK?

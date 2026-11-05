# Handoff: fix/visual-sweep (XC-VIS-001, visual QA of every page)

Worktree `C:\Users\braxt\Desktop\SIDEROOM_wt\visual-sweep`, branch `fix/visual-sweep` (base main `4ae9de5a`, waves
41-46). Not pushed, no PR. Screenshots (outside git): `C:\Users\braxt\Desktop\SIDEROOM_wt\visual-sweep-shots\`.

## Resume here

- **State:** the sweep spec is in (`e2e/tests/page-sweep.spec.ts`, opt-in), 20 defects fixed in the web code, 7 left
  open (below, with reasons). Every route was captured before and after; the final full sweep (608 captures, 152
  routes) is in the screenshot folder, the first one in its `before\` subfolder.
- **Server/DB for the next agent:** scratch DB `sr_visual_sweep` is kept (migrated, Owner bootstrapped, sweep world seeded; `e2e/.state/sweep/world.json` and `owner.json` point at it). Binaries: `C:/Users/braxt/Desktop/SIDEROOM_wt/testdeploy-target/debug/sr-server.exe` and `sr-cli.exe` (CARGO_TARGET_DIR shared with the testdeploy worktree; rebuild with `cargo build -j 2 -p sr-server -p sr-cli`). Env: `SR_DATABASE__URL`, `DATABASE_URL` and `SR_TEST_DATABASE_URL` = the sr_visual_sweep URL (password from `%LOCALAPPDATA%/sideroom/sr_pg_pw.txt`, never printed), `SR_SERVER__PUBLIC_URL=http://localhost:5173`, `SR_STORAGE__ENDPOINT=memory:`, `SR_SERVER__BIND=127.0.0.1:8080`, `SR_CLI_BIN` = the sr-cli.exe above. Start sr-server, wait for `/readyz`; Playwright starts Vite (5173) itself. Nothing is running now.
- **What is left:** (1) run `visual.spec.ts` on Chromium once; (2) owner decisions on O1 and O4; (3) optional fixes for O2, O3, O6; (4) drop `sr_visual_sweep` when the work is done; (5) push / PR only when the owner says so.
- **Next:** owner review of the open items (O1 needs a decision on how a message request opened by link should
  look; O4 is a design question). Push and open a PR when the owner says so.

## How to run the sweep

Stack: `sr-server` on 127.0.0.1:8080 with `SR_DATABASE__URL` (own scratch DB), `SR_SERVER__PUBLIC_URL=http://localhost:5173`,
`SR_STORAGE__ENDPOINT=memory:`; Playwright starts the Vite dev server (`VITE_SR_E2E=1`).

```bash
SR_E2E_SWEEP=1 pnpm --filter @sideroom/e2e exec playwright test tests/page-sweep.spec.ts --project chromium
# a few pages again (regex over route names); the summary merges with what is on disk:
SR_E2E_SWEEP=1 SR_SWEEP_ONLY='^(lounge-channel|arcade)$' pnpm --filter @sideroom/e2e exec playwright test tests/page-sweep.spec.ts --project chromium
# remove the world (its server, title and the storefront switches) afterwards:
SR_E2E_SWEEP=1 SR_SWEEP_CLEANUP=1 pnpm --filter @sideroom/e2e exec playwright test tests/page-sweep.spec.ts --project chromium -g "remove the sweep world"
```

Options: `SR_SWEEP_OUT` (default `<repo>/../visual-sweep-shots`), `SR_SWEEP_ONLY`, `SR_SWEEP_RESEED=1`, `SR_SWEEP_CLEANUP=1`, `SR_SWEEP_DARK=<theme id>` (the "dark" pass in another theme, e.g. `lounge-dark`; default `lounge-editorial`).

What it does:
- **World** (through the API, `e2e/.state/sweep/world.json`): a server "The Back Booth" with a second member and two
  hoisted roles, a voice channel and a long-named text channel; messages with markdown, a mention, a code block, a
  link preview embed, a rich embed with fields and image, an unbroken 100-character word, a reaction, a thread; a DM;
  a moderation report (the `moderation.reports` switch is turned on); a developer application; an approved Activity
  (the member is the developer, the Owner reviews); a TV channel; the a11y world's published title and watch party; the
  storefront switches on. Uploads need object storage, which `memory:` cannot serve to a browser, so the message list
  response is decorated in the page: the `[files]` message gets an image and a PDF attachment, and attachment and
  embed images are answered with a PNG. The Study's shelves are faked like `study-shelves.spec.ts` does.
- **Captures:** every route of `router.tsx` and the per-area route files (Lounge, DMs, threads, scrapbook, voice,
  all 20 user and 20 server settings sections, all 23 admin sections, Cinema incl. player, TV guide and live, watch
  party, shelves, Library, Study, Arcade, emulator, developers incl. all six app tabs and OAuth2, support, profiles,
  help, share, storage, invite/template in the app, two unknown routes) signed in, the public surfaces signed out
  (landing, join, login, 2FA, link, register, forgot, reset, verify, email-change, Discord callback, invites,
  template, no-invitation, public profile, guest play, house notes, the static error pages, unknown paths), and six
  pages with the Showroom switch on. Desktop 1280x800 and phone 375x812; "light" = Lounge Light chosen for
  everything, "dark" = the default a new account gets (Editorial). Full-page PNGs named
  `<route>-<viewport>-<theme>.png`.
- **Per capture** (`data/*.json`, `summary.json`, `summary.md`): document status, final URL, console errors, page
  errors, failed requests (4xx/5xx and network failures, minus the expected ones), error-boundary text, blank main
  area, horizontal overflow, interactive elements outside the viewport (not inside a scroller), text that looks like
  a raw catalog key. Overlap detection was not added (not cheap enough to be reliable); overlaps were found by eye.
- **Contact sheets** (`sheets/`): two routes per image, each row desktop light, desktop dark, phone light, phone dark
  (top of the page), which is how every page was reviewed.

## Defects found and fixed

Before/after: `visual-sweep-shots\before\<file>` and `visual-sweep-shots\<file>`.

| # | Defect (who sees it) | Fix | Before / after |
|---|---|---|---|
| D1 | A long URL or unbroken word ran out of the message column, under the member list (every Lounge channel, all viewports) | `lounge.css`: `.lng-msg-body { overflow-wrap: anywhere }` | `lounge-channel-desktop-light.png` |
| D2 | Avatar initials kept punctuation: "Rowan (owner)" showed "R(" (every avatar without a picture) | `packages/ui` `initials()` takes the first letter or digit of each word; test in `display.test.tsx` | `settings-user-profile-desktop-light.png` |
| D3 | Settings layouts on phones: the floating Close button covered the section title and description (all user/server settings and admin). Wide pages (admin, server settings): header actions (New title, Grant, Send appointment offer, Create invite, Create Channel) sat under Close | `Settings.module.css`: Close in the flow on phones, compact; wide content keeps 112px inline-end padding | `admin-accounts-phone-light.png`, `admin-cinema-catalog-desktop-light.png`, `settings-server-invites-desktop-light.png` |
| D4 | Server Settings, the Arcade, the emulator, The Study and the storage browser ignored the saved theme (dark in Lounge Light) and their background stopped where the content ended (a dark band under the page) | New `settings/user/LoungeThemeProvider.tsx` (applied theme, fills the viewport); lazy in the route files so the first-paint chunk does not grow | `arcade-desktop-light.png`, `settings-server-overview-desktop-light.png`, `study-desktop-light.png` |
| D5 | Profile page editor: the Colours fieldset ran 76px past its 320px column (hex fields off screen, overflow on phones); header links ran together ("Back to the LoungeView page") | `profilepage.css`: one colour per row, styled hex inputs, spaced `.pp-actions` | `profile-page-edit-desktop-light.png`, `profile-page-edit-phone-light.png` |
| D6 | An unknown app address showed one unstyled line, "Page not found.", in the top corner with no way back | `app/NotFoundPage.tsx` (own chunk): themed empty state and "Back to the Lounge"; router keeps a lazy stub | `app-unknown-route-desktop-dark.png` |
| D7 | Invite and template cards (public and in-app) were 8px wider than a phone (horizontal scroll) | `entrance.css`: `box-sizing: border-box` on the page and card | `pub-invite-real-phone-light.png` |
| D8 | Landing and join on phones: the invitation field showed five characters ("ABCD1") beside the button | `entrance.css` (520px and below): the button goes under the field | `pub-join-phone-dark.png` |
| D9 | A thread opened with the member list open (1280px): the thread pane was 300px, its title ran into the header buttons and the composer placeholder broke after every few letters | `threads.css`: below 1600px the member list steps aside while a thread is open (desktop only) | `lounge-thread-desktop-light.png` |
| D10 | Voice channel jukebox strip stayed a dark box in Lounge Light ("Music volume" unreadable) | `jukebox.css`: `--ui-*` tokens first, `--sr-*` as fallback | `lounge-voice-desktop-light.png` |
| D11 | Showroom banner: mounted outside every ThemeProvider, so it had no background or border; its text ran over the page, and on phones over the tab bar and composer | `showroom.css`: `--sr-*` fallbacks and a shadow, bottom inline-end above the composer; a compact strip on phones | `showroom-home-phone-dark.png`, `showroom-channel-desktop-light.png` |
| D12 | A channel with attachments or embeds opened scrolled up, on "You're viewing older messages" (rows and media size in after the first jump to the end) | `MessageList.tsx`: for 2.5 s after opening (ended by wheel, touch or key), or while at the bottom, a change of the list height keeps the newest message in view | `lounge-channel-desktop-dark.png` |
| D13 | Cinema and Library section bar on phones: two navs each scrolling on their own squeezed each other ("The Arca" cut off next to "Shelves") | `cinema.css`, `library.css`: the bar scrolls as one | `cinema-home-phone-light.png`, `library-home-phone-light.png` |
| D14 | Cinema rows on phones: "Explore all" wrapped to two lines next to the arrows; the label was a hard-coded English default | `Poster.module.css`: tools do not wrap; `Discover.tsx` passes `t('cinema.discover.exploreAll')` | `cinema-home-phone-light.png` |
| D15 | Cinema "Submit a video" and a submission page had no page padding (heading on the edge) | `cin-page` on both | `cinema-submit-desktop-light.png` |
| D16 | Watch party "not available": the description repeated the title | `watchparty/pageMessage.ts` drops a description equal to the title; `pageMessage.test.ts` | `cinema-party-desktop-light.png` |
| D17 | TV guide: "My reminders" sat flush under the grid and its select was an unstyled native control touching the text | `tv.css` | `cinema-tv-desktop-light.png` |
| D18 | User Settings > Privacy opened by link: "You are not in any servers" for a member of three (the store is empty without the Lounge's connection) | `SafetyCards.tsx`: falls back to `GET /users/@me/guilds` | `settings-user-privacy-desktop-light.png` |
| D19 | Rows in admin and server settings on phones: switches and buttons squeezed the name and description to one word per line (Admin > Feature modules) | `server-settings.css` (720px and below): text keeps 12rem, controls wrap under it | `admin-features-phone-light.png` |
| D20 | When the rest of the message catalog failed to load once (a dropped request), every app screen showed raw keys (`settings.bans.title`, `admin.adminApp.section.jobs`) until a reload. Seen three times in the sweep under load | `i18n/runtime.ts`: tried again after 0.6 s before the gates open degraded, then at 2, 5 and 15 s in the background (the text replaces the keys when it arrives); `catalogRetry.test.ts` | `admin-invite-graph-desktop-dark.png` (before) |

## Still open

| # | What | Why not fixed |
|---|---|---|
| O1 | `/channels/@me/<id>` for a DM that is still a pending message request (the member wrote first) shows "This channel doesn't exist" to the recipient; the DM list says "No conversations yet" while Message Requests shows a badge | Behaviour, not layout: whether a request opened by link should show the request (accept/ignore) is a product decision. Owner question. |
| O2 | TV guide: the first time column label is cut at the start (":00 AM") | The grid scrolls to "now"; the label belongs to a slot that starts before the visible window. Minor; needs a sticky label in the guide grid, not a CSS tweak. |
| O3 | Server Settings > Channels on phones: long channel rows and "Add channel" run past the right edge of the list | The channel tree is a fixed-width editor with drag handles; making it fluid is a component change, not a sweep fix. |
| O4 | The developer portal stays dark in Lounge Light | `DevelopersRoot` passes only the per-section theme (section `developer`, default Lounge Dark) on purpose; owner/design call whether it should follow the global choice. |
| O5 | `/404.html` and `/50x.html` cannot be reached in the Vite dev topology (the dev gate sends them to `/login`) | Production serves them (Caddy and `web.rs`); checked from the files instead: plain until `/offline.css` loads, which `offline.html` shows styled. |
| O6 | On phones the Lounge's off-canvas panes (server rail, DM list) stay in the tab order while off screen (the sweep's "offscreen" check lists them) | Accessibility, not visual; needs `inert` on the hidden pane, which touches keyboard specs. |
| O7 | Cinema, Library, help and the public pages do not follow Lounge Light | By design: they have their own section themes (Side Room), selectable per section in Appearance. Not a defect. |

Environment notes (not app defects): the first capture after Vite starts can be blank (cold compile; re-run shows the
page); under load Vite dropped module requests a few times, which is how D20 was found; `lounge-thread` once hit the
API rate limit (429) because the sweep opens many pages in a short time.

## XC-VIS-003: owner-delegated polish decisions (Fable, 2026-11-02)

Fable's decisions on the XC-VIS-002 owner questions (`polish-fable-decisions.md`, outside git), recorded as P30-VIS-1
in the decisions log. Implemented, one commit each (`XC-VIS-003 <id>: ...`):

- **S5 (visible in every dark Lounge, for the CHANGELOG):** links in running text are #00a8fc in Lounge Dark and
  Onyx and #1ab2ff in Ash (the brand blue was 2.5 to 3:1). Rules in `styles/app-links.css` (app only, D-385);
  `styles/__tests__/textLink.test.ts` checks every Lounge theme against its three surfaces. L2 (the Lounge button
  reset) is the other visible Lounge change of this round.
- **G5:** with an Activity running in the channel, the composer's "+" item and the launcher offer Join, not Start.
- **T4:** Server Settings > Members: Roles stays a button; Timeout, Kick and Ban move to a per-row "..." menu (the UI
  kit menu items gained optional `title` and `ariaLabel`).
- **M3:** a not-ready or failed video goes "Back to the title page"; taken-down and missing titles keep /cinema.
- **S3:** the login-link, verify-email and email-change-cancel pages say in the headline when the link failed or
  finished (`entrance.linkOutcome.*`, core entrance catalog).
- **G2 / G3:** the emulator page is titled "Emulator"; Showroom section headings are sentence case, the card badges
  stay capitals through CSS.
- **M5:** the storage browser's Back goes to the settings page that opened it (`?back=`, only `/settings/...`).
- **T6:** the settings polish flows run serially; the admin switch flow skips when `SR_SWEEP_SHARED=1`.
- **Not done: S6** (register `--text-link` in the sr-theme registry, move the values into `themes.ts`): needs Rust,
  a follow-up PR after #82.

Environment note: the web production build rewrites `apps/web/node_modules/.vite/deps`, which the running Vite dev
server shares; afterwards every app route captured blank (504 "Outdated Optimize Dep"). Touching
`apps/web/vite.config.ts` makes Vite reload in place and re-optimize; do that after a build while the dev server runs.

## Gates

| Gate | Result |
|---|---|
| `tsc --noEmit` web, e2e | 0 errors |
| eslint (web: `eslint .`; e2e spec) | clean |
| prettier `--check .` | clean except the gitignored `e2e/.state` files the run writes |
| `i18n-audit` | TOTAL 0 (new fragment `xc-vis-1.ts`) |
| `check-css-tokens` | 69 files clean |
| vitest web, full, `--maxWorkers=2` | 448 files, 4066 tests pass |
| vitest `packages/ui` | 15 files, 330 tests pass |
| `build-web.mjs`; bundle budget | ok; app (first-paint) chunk 248.4 KB, was 248.2 (+0.2 KB of the 1.8 KB headroom: the not-found stub, the catalog retry, CSS); every surface within budget |
| entrance isolation | every public surface clean |
| Playwright `page-sweep.spec.ts` (Chromium) | full run 616 passed (608 captures) on the fixed tree |
| Playwright `visual.spec.ts` (Chromium) | not run (checkpoint requested before it; no page it covers changed at 1280x800) |
| Rust | not touched, not run |

Visual baselines (`e2e/tests/visual.spec.ts-snapshots`) were not changed: they are desktop 1280x800 captures of
entrance, Cinema and Library pages, and no fix changes those pages at that width.

## Routes checked

| Route | Path | Viewports x themes | HTTP | Session | Status |
|---|---|---|---|---|---|
| admin | `/admin` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-accounts | `/admin/accounts` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-bans | `/admin/bans` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-cinema-catalog | `/admin/cinema-catalog` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-cinema-collections | `/admin/cinema-collections` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-cinema-live-tv | `/admin/cinema-live-tv` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-cinema-transcoding | `/admin/cinema-transcoding` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-dashboard | `/admin/dashboard` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-data-requests | `/admin/data-requests` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-donation-entitlements | `/admin/donation-entitlements` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-donation-invoices | `/admin/donation-invoices` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-donation-pricing | `/admin/donation-pricing` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-donations | `/admin/donations` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-features | `/admin/features` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D19) |
| admin-ingest-sources | `/admin/ingest-sources` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-invite-graph | `/admin/invite-graph` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-invites | `/admin/invites` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-jobs | `/admin/jobs` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-library-catalog | `/admin/library-catalog` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-moderation | `/admin/moderation` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-review | `/admin/review` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-servers | `/admin/servers` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-staff | `/admin/staff` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| admin-storage | `/admin/storage` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| app-404 | `/channels/@me/does/not/exist/here` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok (correct "This channel doesn't exist" state) |
| app-door | `/app` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| app-unknown-route | `/nothing-lives-here` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D6) |
| arcade | `/games` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D4) |
| cinema-anime | `/cinema/anime` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D13) |
| cinema-home | `/cinema` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D13, D14) |
| cinema-movies | `/cinema/movies` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D13) |
| cinema-my-list | `/cinema/my-list` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D13) |
| cinema-party | `/cinema/party/<id>` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D16) |
| cinema-player | `/cinema/watch/<id>` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| cinema-search | `/cinema/search?q=film` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D13) |
| cinema-series | `/cinema/series` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D13) |
| cinema-shelves | `/cinema/shelves` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D13) |
| cinema-submit | `/cinema/submit` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D15) |
| cinema-title | `/cinema/t/<id>` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D13) |
| cinema-tv | `/cinema/tv` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | D17 fixed; O2 open |
| cinema-tv-watch | `/cinema/tv/watch/<id>` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D13) |
| developers | `/developers` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | open (O4) |
| developers-app-bot | `/developers/applications/<id>?tab=bot` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| developers-app-commands | `/developers/applications/<id>?tab=commands` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| developers-app-general | `/developers/applications/<id>?tab=general` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| developers-app-installation | `/developers/applications/<id>?tab=installation` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| developers-app-oauth2 | `/developers/applications/<id>?tab=oauth2` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| developers-app-review | `/developers/applications/<id>?tab=review` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| developers-applications | `/developers/applications` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| developers-doc-page | `/developers/docs/getting-started` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| developers-docs | `/developers/docs` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| developers-packages | `/developers/packages` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| emulator | `/games/emulator` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D4) |
| help | `/help` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| help-operators | `/help/operators` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| help-whats-new | `/help/whats-new` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| invite-app | `/invite/<code>` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D7) |
| library-browse | `/library/browse` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D13) |
| library-home | `/library` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D13) |
| library-shelves | `/library/shelves` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D13) |
| library-submissions | `/library/submissions` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D13) |
| lounge-channel | `/channels/<id>/<id>` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D1, D2, D12) |
| lounge-dm | `/channels/@me/<id>` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | open (O1) |
| lounge-home | `/channels/@me` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D2) |
| lounge-message-link | `/channels/<id>/<id>/<id>` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D1, D12) |
| lounge-scrapbook | `/channels/<id>/scrapbook` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D2) |
| lounge-thread | `/channels/<id>/<id>/threads/<id>` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D1, D9) |
| lounge-voice | `/channels/<id>/<id>` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D10) |
| oauth2-authorize | `/oauth2/authorize?client_id=<id>&scope=bot%20applications.commands&permissions=2048` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| oauth2-authorized | `/oauth2/authorized` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| profile-member | `/u/<member>` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D5) |
| profile-page-edit | `/profile-page/edit` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D5) |
| profile-tilde-app | `/~owner` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D5) |
| profile-u | `/u/owner` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D5) |
| pub-404-html | `/404.html` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | open (O5) |
| pub-50x-html | `/50x.html` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | open (O5) |
| pub-app-route-signed-out | `/channels/@me` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-discord-callback | `/oauth/discord/callback` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-email-change-cancel | `/email-change/cancel` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-forgot | `/forgot` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-house-notes | `/house-notes` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-invite-bad | `/invite/abcd1234` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | fixed (D7) |
| pub-invite-real | `/invite/<code>` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | fixed (D7) |
| pub-join | `/join/abcd1234` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | fixed (D8) |
| pub-landing | `/` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | fixed (D8) |
| pub-login | `/login` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-login-2fa | `/login/2fa` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-login-link | `/login/link` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-no-invitation | `/no-invitation` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-offline-html | `/offline.html` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-play | `/play/ab12cd34ab12cd34ab12cd34ab12cd34ab12cd34ab12cd34ab12cd34ab12cd34` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-play-bad | `/play/not-a-token` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-profile | `/~owner` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-profile-missing | `/~nobody-here` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-register | `/register` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | fixed (D8) |
| pub-reset | `/reset/dummytoken` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-template | `/template/abcd1234` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | fixed (D7) |
| pub-unknown | `/login/nothing-here` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| pub-verify-email | `/verify-email/dummytoken` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | out | ok |
| settings-server | `/settings/server/<id>` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-audit-log | `/settings/server/<id>/audit-log` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-bans | `/settings/server/<id>/bans` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-bridge | `/settings/server/<id>/bridge` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-channels | `/settings/server/<id>/channels` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | D3, D4 fixed; O3 open |
| settings-server-community | `/settings/server/<id>/community` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-delete | `/settings/server/<id>/delete` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-emoji | `/settings/server/<id>/emoji` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-extensions | `/settings/server/<id>/extensions` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-features | `/settings/server/<id>/features` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-gameservers | `/settings/server/<id>/gameservers` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-integrations | `/settings/server/<id>/integrations` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-invites | `/settings/server/<id>/invites` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-members | `/settings/server/<id>/members` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4, D2) |
| settings-server-moderation | `/settings/server/<id>/moderation` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-overview | `/settings/server/<id>/overview` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-roles | `/settings/server/<id>/roles` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-storage | `/settings/server/<id>/storage` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-support | `/settings/server/<id>/support` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-tag | `/settings/server/<id>/tag` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-server-watch-library | `/settings/server/<id>/watch-library` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D4) |
| settings-user-accessibility | `/settings/user/accessibility` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-account | `/settings/user/account` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D2, D3) |
| settings-user-activity | `/settings/user/activity` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-advanced | `/settings/user/advanced` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-appearance | `/settings/user/appearance` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-apps | `/settings/user/apps` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-connections | `/settings/user/connections` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-devices | `/settings/user/devices` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-invites | `/settings/user/invites` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-keybinds | `/settings/user/keybinds` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-language | `/settings/user/language` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-modules | `/settings/user/modules` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-notifications | `/settings/user/notifications` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-privacy | `/settings/user/privacy` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3, D18) |
| settings-user-profile | `/settings/user/profile` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D2, D3) |
| settings-user-storage | `/settings/user/storage` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-streamer | `/settings/user/streamer` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-support | `/settings/user/support` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-text | `/settings/user/text` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| settings-user-voice | `/settings/user/voice` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D3) |
| share | `/share` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| showroom-arcade | `/games` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | showroom | fixed (D4, D11) |
| showroom-channel | `/channels/<id>/<id>` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | showroom | fixed (D1, D11) |
| showroom-cinema | `/cinema` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | showroom | fixed (D11, D13) |
| showroom-home | `/channels/@me` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | showroom | fixed (D11) |
| showroom-settings | `/settings/user/appearance` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | showroom | fixed (D3, D11) |
| showroom-study | `/study` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | showroom | fixed (D4, D11) |
| storage-mount | `/storage/123456` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D4) |
| study | `/study` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D4) |
| support | `/support` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| support-leaderboards | `/support/leaderboards` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | ok |
| template-app | `/template/abcd1234` | 4 (desktop 1280x800, phone 375x812; light, dark) | 200 | in | fixed (D7) |


# Wave 87, public builder: SEC-AUD-364 (D-886)

T = 16:40 on 2026-11-16 (no new item 17:10, checkpoint 17:22, finished 17:30, hard limit 17:40). I started preparing at 16:26 by the machine's clock and finished at 16:41.

## Done
A profile page's block headings (`h3.pp-title`) take the profile's own text colour, `var(--pp-text)`, the variable its body text uses. Borders, marks and links keep the accent. One rule, in `apps/web/src/profilepage/profilepage.css`:
`.pp-root .pp-title { color: var(--pp-text); }` (no hex, no token, no theme name). One import line in `profilepage/ProfileRenderer.tsx` (`import './profilepage.css'`) so the rule travels with the renderer to every surface that uses it (the pop-up and the Settings preview did not load that sheet by themselves; they only saw it when the page had been opened earlier in the session). No `.tsx` logic changed, so no new vitest case.

## Where the colour came from (a deviation from the plan, listed for the commander)
Not in my files. The base colour is `.pp-title { color: var(--pp-accent) }` in `packages/profile-blocks/src/profile-blocks.css` (the shared renderer package; `packages/**` is fenced this wave). No inline style anywhere. I therefore added the override in my own sheet (specificity 0,2,0 beats the package's 0,1,0 whatever the order). **The cleaner fix is the one line in the package (`color: var(--pp-text)` in `.pp-title`)**; then my rule and the import can go. **Not covered by my change: the signed-out public page `/~handle` (`apps/web/src/public/profile/PublicProfile.tsx`), which renders the package renderer directly and does not load `profilepage.css`.** I did not probe it (public profiles need a feature flag on); it has the same package rule, so it is presumably still 2.99:1 with the default accent. Proposed: one-line package change in a later wave (or the commander allows it now).

## What I saw on the base (run: `SR_P87_PUBLIC_BASE=1`, change not yet made)
Owner's page `/u/owner` of the sweep world (headings About me, Support me, Guestbook; default accent #5865f2), ground `section.pp-block` rgb(43,45,49), 13 px:
- 1280 Editorial, 375 Lounge Dark, 375 Lounge Light: every heading rgb(88,101,242), **2.99:1**. Pop-up from the Member List (1280): About me and Support me, 2.99:1. The fault is as the plan says.
- The Settings preview was not probed on the base (my first spec version looked for the wrong element); its renderer is the same, wave 83 measured 2.99:1.

## After (spec `SR_P87_PUBLIC_ON=1`, same world)
| surface | default accent | other accent (#e91e63) |
|---|---|---|
| page 1280 Editorial | 12.42:1 (rgb 242,243,245 on 43,45,49) | 12.42:1 |
| page 375 Lounge Dark | 12.42:1 | 12.42:1 |
| page 375 Lounge Light | 12.42:1 | 12.42:1 |
| Settings preview (Profile tab, "View as" region) at 1280, 375 Dark, 375 Light | 12.42:1 | not run |
| pop-up from the Member List, 1280 | 12.42:1 | not run |
All headings listed per run in `visual-sweep-shots/p87-public/*-measure.json`. 0 px moved: every block box and every heading box, before vs after, identical on all three page settings and on the pop-up; no sideways scroll (0). The other accent is not a seeded member's: the spec sets it on the Owner's own document and sets the original back. The pop-up on a phone (Member List pane) was not tried.

## Build and checks (the wave's one web build, `heavy.sh run public`, 16:39 to 16:40)
Build exit 0. `check-bundle-budget` exit 0: first paint `app/app-D07Eqehr.js` **218 310 B gzip** (before, wave 86: 218 312; 2 B less), room 37 690 B; `profile` entry 115.1 KB / 150 KB. `check-entrance-isolation`: "every public surface is clean", exit 0. `check-staff-delivery` exit 0.

## Tests run
- Vitest `profilepage/__tests__/page.test.tsx`: 15 passed, as it is. (Watched fail without the change: no, it does not assert a colour; the spec's base case is the failing check: 2.99:1.)
- Web `tsc --noEmit` clean; eslint on `ProfileRenderer.tsx` clean; prettier clean on css, tsx and spec; `check-css-tokens` clean; `i18n-audit --max 0` TOTAL 0; e2e `tsc` clean; spec run with `--project=chromium --reporter=line`: 3 runs passed (base, after, after with other accent). `check-doc-links` run at the end (see commit).

## Shots (folder `visual-sweep-shots/p87-public`, 15 PNG, all opened by me)
before: page 1280 Editorial, 375 Dark, 375 Light, pop-up 1280 (4). after: page x3, Settings preview x3, pop-up (7). after-accent: page x3 (3). Plus `before-`, `after-`, `after-accent-measure.json`. Observed but not mine and not changed: in Lounge Light the page's guestbook label "LEAVE A MESSAGE" and the disabled "Sign" button read faint on the dark panel.

## Commander's command (also in the spec header)
From `e2e/`, on a stack with the sweep's seeded world (Vite 5651, API 8651): `SR_P87_PUBLIC_ON=1 SR_P87_PUBLIC_TAG=after SR_P87_PUBLIC_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p87-public SR_E2E_BASE_URL=http://localhost:5651 npx playwright test tests/p87-public-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (`SR_P87_PUBLIC_BASE=1` for the base case; add `SR_P87_PUBLIC_ACCENT=1` for the other accent).

## Docs
No doc states the heading colour. Nothing updated.

## Proposed closing text for SEC-AUD-364
Closed (wave 87, D-886): a profile page's block headings take the profile's own text colour (`--pp-text`) on the page, in the Settings preview and in the pop-up: 12.42:1 with the default and another accent (was 2.99:1). Open: the signed-out `/~handle` page (`PublicProfile`, package CSS) keeps the accent heading until `packages/profile-blocks/src/profile-blocks.css` `.pp-title` is changed (one line).

## Changelog sentence
A profile page's section headings now use the page's own text colour, so they read clearly whatever accent the member picked.

## Owner questions
Allow the one-line change in `packages/profile-blocks` (and then drop the override and the import in `ProfileRenderer.tsx`) so the signed-out public page is fixed too?

## Clean-up
Stack stopped by process tree, ports 8651/5651/9351 free, `sr_p87_public_ui` dropped, `p87-public-bin` deleted (`p80-base-bin` untouched), lock released. No stash touched.

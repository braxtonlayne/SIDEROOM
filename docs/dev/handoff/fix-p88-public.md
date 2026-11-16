# Wave 88, public builder: SEC-AUD-364 finished (D-891) and the D-894 figures

T = 16:55 on 2026-11-16 (hard limit 17:45). Clock: started 16:53, code and spec ready 16:56, lock taken 16:59, lock released 17:01, web build 17:01 to 17:04 (one-shot `heavy.sh run public`), finished 17:07.

## Done (commits on `fix/p88-public`)
- `b798f3e6` SEC-AUD-364: `packages/profile-blocks/src/profile-blocks.css`, `.pp-title` rule only: `color: var(--pp-text)` in place of the accent, one comment line. `apps/web/src/profilepage/profilepage.css`: the wave 87 `.pp-root .pp-title` rule and its two comment lines removed. `apps/web/src/profilepage/ProfileRenderer.tsx`: the `import './profilepage.css'` removed.
- The spec `e2e/tests/p88-public-shots.spec.ts` (copied from wave 87's, plus the guestbook measure and the signed-out page).

## The import: why it went
`git log -S"profilepage.css" -- ProfileRenderer.tsx`: wave 87's commit `7782dbfc` added it, for this rule only. `ProfileRenderer.tsx` has no `className` of its own (everything it renders is a package class), and `profilepage.css` holds only the page chrome and the editor, which `ProfilePageView.tsx` and `ProfileEditor.tsx` each import themselves. Nothing else needed it there. The pop-up and the Settings preview render correctly without it (shots below).

## What I saw on the base (run, `SR_P88_PUBLIC_BASE=1`)
The base for the in-app page is wave 87's tree (override in place): 12.42:1 on the page, preview and pop-up, as wave 87 said (I ran it: the three package/web files checked out from `HEAD~1`, restored after). The signed-out `/~owner` page on that base: every heading rgb(88,101,242) on rgb(43,45,49), **2.99:1** (About me, Support me, Guestbook). So the fault was exactly the one wave 87 listed.

## After (`SR_P88_PUBLIC_ON=1`, default accent #5865f2, and once with #e91e63)
| surface | ratio (rgb(242,243,245) on rgb(43,45,49)) |
|---|---|
| page 1280 Editorial | 12.42 (all three headings); other accent 12.42 |
| page 375 Lounge Dark | 12.42; other accent 12.42 |
| page 375 Lounge Light | 12.42; other accent 12.42 |
| Settings preview (Profile tab) 1280 Editorial, 375 Dark, 375 Light | 12.42 (About me, Support me) |
| pop-up from the Member List, 1280 | 12.42 (About me, Support me) |
| **signed-out `/~owner` at 375 (package sheet only)** | **12.42** (About me, Support me, Guestbook; was 2.99) |

0 px moved: every block box and heading box identical to the base on all three settings, and the signed-out page's block boxes identical; no sideways scroll (0).

**The signed-out page was reachable:** the sweep's seeded world already has `platform.public_profiles` on (`PATCH /admin/features/platform.public_profiles {enabled:true,reason}`), `profiles.access = opt_in_public` (`PATCH /admin/settings`) and the Owner's `PUT /users/@me/profile-privacy` with `page_tier: 'public'`; `/~owner` opens with no session.

## Guestbook, D-894 (measured only, nothing built): `/u/owner` at 375 in Lounge Light
The profile's panel is dark (rgb 43,45,49) whatever the app theme, so app tokens that follow the theme land on it wrong.
- "Leave a message" label (`label._label_12gqi_6`, 12 px, uppercase): colour rgb(92,94,102), opacity 1, ground `section.pp-block` rgb(43,45,49), **2.13:1**. Set by the kit: `packages/ui/src/primitives/Input/Input.module.css` `.label { color: var(--ui-text-muted) }` (a kit rule; the package sheet only styles `.pp-sign > label` for layout).
- Disabled "Sign" button (`button.primary.sm`, 13 px): text rgb(255,255,255), opacity 0.5 (kit `.button:disabled { opacity: 0.5 }`, Button.module.css:23), ground the accent rgb(88,101,242), effective **2.3:1**. Kit rule; a disabled control is exempt from WCAG 1.4.3 but reads faint.
- The textarea: rgb(49,51,56) on rgb(242,243,245), 11.39:1 (fine; in Lounge Light the field is a light box on the dark panel).
Proposed fix (a later wave, package sheet only, no kit change): `.pp-sign label { color: color-mix(in srgb, var(--pp-text) 75%, transparent); }` (0,1,1 beats the kit's 0,1,0), and `.pp-sign button:disabled { opacity: 1; background: color-mix(in srgb, var(--pp-accent) 45%, transparent); color: var(--pp-text); }`. Fails D-894: yes for the label (2.13:1); the button is a disabled state.

## Build and checks (web build 17:01 to 17:04, exit 0)
`check-bundle-budget` exit 0: first paint `app/app-DYz333k8.js` **218 317 B gzip** (wave 87: 218 310; +7 B), room 37 683 B (needs 8 KB); `profile` entry 115.1 KB / 150 KB. `check-entrance-isolation` exit 0, "every public surface is clean". `check-staff-delivery` exit 0.

## Tests run
- `packages/profile-blocks` `blocks.test.tsx`: 21 passed. `apps/web` `profilepage/__tests__/page.test.tsx`: 15 passed (both as they are; neither asserts a colour; the spec's base run is the check: signed-out 2.99 before, 12.42 after).
- Web `tsc --noEmit` clean (also the first step of the build); eslint on `ProfileRenderer.tsx` clean; prettier clean on the css, tsx and spec; `check-css-tokens` clean; `i18n-audit --max 0` TOTAL 0; e2e `tsc` clean; `check-doc-links` ok; the spec ran on the stack: base, after, after with other accent, all passed (`--project=chromium --reporter=line`).

## Shots (`visual-sweep-shots/p88-public`, 21 PNG plus `before-`, `after-`, `after-accent-measure.json` and `build.log`)
I opened 9 of them: the base page 375 Dark, the signed-out page before and after, the after page at 1280 Editorial, 375 Dark and 375 Light, the Settings preview (1280), the pop-up and the guestbook shot. The rest (the base's other copies, after-accent x3, the phone previews) were not opened; their numbers are in the JSON. The guestbook shot is the viewport scrolled to the form, not a tight crop.

## Docs
No doc states the heading colour. Nothing updated.

## Proposed closing text for SEC-AUD-364
Closed (wave 88, D-891): a profile's block headings take the profile's own text colour (`--pp-text`) at the source, in `packages/profile-blocks`, so the page, the Settings preview, the pop-up and the signed-out `/~handle` page all read 12.42:1 with the default and another accent (was 2.99:1). The wave 87 override and import are gone.

## Proposed row for the guestbook
SEC-AUD-382 (proposed): the profile guestbook's "Leave a message" label reads 2.13:1 on the profile's dark panel in Lounge Light (kit `--ui-text-muted` on a panel that does not follow the theme); the disabled "Sign" button is the kit's 0.5 opacity (2.3:1). Fix in the package sheet as above.

## Changelog sentence
A profile's section headings now use the page's own text colour on every surface, including the public `/~handle` page, so they read clearly whatever accent the member picked.

## Owner questions
Should the guestbook form on a profile take the profile's own colours (label and disabled button), as proposed above?

## Commander's command (also in the spec header)
From `e2e/`, on a stack with the sweep's seeded world (Vite 5661, API 8661, metrics 9361): `SR_P88_PUBLIC_ON=1 SR_P88_PUBLIC_TAG=after SR_P88_PUBLIC_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p88-public SR_E2E_BASE_URL=http://localhost:5661 npx playwright test tests/p88-public-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line` (`SR_P88_PUBLIC_BASE=1` for the base case; add `SR_P88_PUBLIC_ACCENT=1` for the other accent).

## Clean-up
Stack stopped by PID tree, ports 8661/5661/9361 free, `sr_p88_public_ui` dropped, `p88-public-bin` deleted (`p80-base-bin` untouched), lock released. `apps/web/dist` is ignored build output. No stash touched.

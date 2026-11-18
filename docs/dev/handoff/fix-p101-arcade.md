# Wave 101, arcade: the Coming soon Repository link and the tier in Admin > Arcade (SEC-AUD-445, SEC-AUD-446, D-956)

Builder: arcade. Branch `fix/p101-arcade`. Clock (machine clock): started 06:20; the lock was held by other builders until 07:19 (my `acquire` loop kept trying); round 07:19 to 07:22; handoff 07:25. No Rust, no web build, no push.

## Done
- `a.arc-link` (the Coming soon card's Repository link) is a 44 px target on a phone: `apps/web/src/arcade/arcade.css`, inside the sheet's existing `@media (max-width: 40rem)` block, right after the hero's `.arc-actions :is(button, a) { min-block-size: 44px }` rule: `.arc-link { display: inline-flex; align-items: center; min-block-size: 44px; }` with a comment naming SEC-AUD-445 and D-956. 40rem (640 px) is the sheet's own phone width and the one its other 44 px rule uses (the 60rem query is for the hero grid only). Nothing above 40rem changes. `git grep` finds `arc-link` used once (`ArcadeSections.tsx:441`).
- Admin > Arcade lists each Coming soon entry's tier: `ArcadeAdmin.tsx`, `<strong>{s.title}</strong> <span className="arc-chip">{t(tierKey(s.tier))}</span>`, as the candidates line does. No new string, class or fragment.
- Unit test `arcadeAdmin.test.tsx`: "shows the tier of a Coming Soon entry beside its title" (the fixture's `TIER_2_REIMPLEMENTATION` shows "Clean-room reimplementation"). Watched fail on the base (4 passed, 1 failed), then pass after the change (5 of 5).
- Spec `e2e/tests/p101-arcade-shots.spec.ts` (new; its header has the commander's command). One run measures one phase (`SR_P101_ARCADE_PHASE=base|after`).
- Commit `598343cc` holds the code, the test and the spec (subject `SEC-AUD-445, SEC-AUD-446`: one small change for both rows; split it if the commander wants two). A second commit holds this handoff.

## Not done
- `ArcadeSections.tsx`'s two raw selects, the 28 px buttons at 1024 and the card layout: not this wave, untouched.
- The Admin release form's two 28 px buttons (Release, Remove) at 375 (Release fails its below point, taker the Remove button) are the same in base and after: not caused by this change; see proposed rows.

## Figures
One lock round, one database `sr_p101_arcade_ui`, the sweep world seeded fresh (no FAIL line). Base = `arcade.css` and `ArcadeAdmin.tsx` put back from `git show HEAD~1:<path>` copies kept in my scratchpad (never a stash), then the edited files put back; `git status` clean afterwards. Two entries (one `TIER_1_ORIGINAL`, one `TIER_2_REIMPLEMENTATION`) seeded with `seedActivityTier`, picked by the titles it returned, removed in a `finally`. The account theme was read back before each scenario; `data-theme` was read from the Arcade root (`lounge-editorial`; `lounge-light` for the light shot) and from the Admin root (`lounge-light`, `lounge-dark`). At the end the account's settings were put back (theme `lounge-light`, `sectionThemes.arcade = 'lounge-editorial'`, read back).

`/games`, Coming soon, 375 x 812 touch (both seeded cards; Lantern Keep, the world's own entry, has the same link):

| | base | after |
|---|---|---|
| Repository link box | 72.1 x 17 | 72.1 x 44 |
| hit test, above and below points | fail, both taken by `div.arc-card-body` | pass, no taker |
| `div.arc-actions` | 191.5 x 28 | 191.5 x 44 (+16) |
| card (original / reimplementation) | 337.5 x 217.2 / 275.2 | 337.5 x 233.2 / 291.2 (+16 each) |
| "Notify me" | passes | passes |
| link and Notify overlap | 0 | 0 |
| `scrollWidth` / `clientWidth` | 375 / 375 | 375 / 375 |

The card grew by 16 px, the actions row's growth (28 to 44); the link's own +27 is absorbed because the row was already 28 tall. Nothing else in the card moved. Same figures with `sectionThemes.arcade = 'lounge-light'`.
At 1024 x 768 and 1280 x 800 the link (72.1 x 17), `.arc-actions` and the card widths are equal to the base (card heights differ between the two runs only through the random seed titles wrapping differently). Sideways scroll 1024/1024 and 1280/1280, both phases. The hit rows at 1024 and 1280 are informational (a mouse page): the link and "Notify me" fail the 44 px rule there in base and after alike.

Admin > Arcade (the list item's `textContent`):
- Base: the title only. After: `<title> Original...` and `<title> Clean-room reimplementation...`: each entry shows its own tier (the Lantern Keep entry shows "Original").
- 1024 x 768: `li` 545.3 x 121 to 545.3 x 124.8 (+3.8, the chip's line box); release form 545.3 x 95 in both.
- 375 x 812: original entry `li` 161 to 164.8 (+3.8, the chip sits on the title's line); reimplementation entry 161 to 184.8 (+23.8: the long chip "Clean-room reimplementation" wraps under the title); release form 305.9 x 135 in both; sideways 375/375.
- Hit test over `.arc-admin` at 375: 18 controls and 3 fails in base and after (Release, three entries, the below point taken by the Remove button: the two 28 px buttons stack without a gap, same in base). At 1024 equal as well (18 controls; the Select's above/below points taken by its form, and the Release button).
- Crowding: the chip wrapping under a long title at 375 is mine (the base has the title alone). It overlaps nothing: the form's box is unchanged and the shot shows the chip clear of it.

## Who imports `arcade.css`
`ArcadePage` (lazy: `emu/routes.tsx`, `lazyNamed(() => import('../arcade/ArcadePage'))`), `ArcadeShell` (lazy through the same routes), `AdminApp` (lazy: `admin/StaffDoor.tsx`, dynamic `import('./AdminApp')`), `showroom/PreviewArcade` (lazy from `ArcadePage`). None is first paint or a public page, so no web build was run. `ArcadeAdmin.tsx` is in the Admin chunk.

## Shots
8, in `C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-arcade`, all 8 opened and looked at: `base-soon-375-editorial.png`, `soon-375-editorial.png`, `soon-375-lounge-light.png`, `soon-1024-editorial.png`, `base-admin-arcade-375-lounge-light.png`, `admin-arcade-375-lounge-light.png`, `admin-arcade-1024-lounge-light.png`, `admin-arcade-1024-lounge-dark.png` (plus `measure.json`, one JSON line per scenario and phase). What I saw: the link sits in a 44 px row aligned with "Notify me", nothing overlapping or cut; Admin shows the chip beside the title (under it at 375 for the long label); Lounge Light and Dark read the same.

## Checks (own tree, commit `598343cc`; none of the checked files changed after)
- vitest `src/arcade/__tests__/arcadeAdmin.test.tsx`: 5 of 5 pass (06:21); the new test failed on the base first (06:20).
- web tsc: exit 0 (06:23). eslint on `ArcadeAdmin.tsx` and the test: exit 0 (06:23). prettier --check on the four files: exit 0 (07:22). e2e `npx tsc --noEmit -p tsconfig.json`: exit 0 (07:22). `node scripts/check-css-tokens.mjs`: exit 0 (07:22). `node scripts/i18n-audit.mjs --max 0`: TOTAL 0 (07:22). `node scripts/check-doc-links.mjs`: ok, exit 0 (07:22).
- Playwright: base run passed and after run passed (07:21, one test each, about 26 s). The after run asserts each entry's tier label in the list item.

## Docs
Searched `docs/` and `README.md` for the Coming soon list in Admin > Arcade: no doc states what a list item shows; none updated.

## The commander's command (whole; from `e2e/`, after the sweep world is seeded on the same database)
```
SR_P101_ARCADE_ON=1 SR_E2E_BASE_URL=http://localhost:5783 SR_CLI_BIN=<sr-cli> SR_DATABASE__URL=<db> \
DATABASE_URL=<db> SR_P101_ARCADE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-arcade \
npx playwright test tests/p101-arcade-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
Stack ports 8783 / 5783 / 9483, `SR_ADMIN_REQUIRE_REAUTH=0`. For a base comparison put `arcade.css` and `ArcadeAdmin.tsx` back to the base text and add `SR_P101_ARCADE_PHASE=base`. The spec's rows are its own (D-942) and are removed in a `finally`.

## Closing texts
- SEC-AUD-445: closed. The Coming soon card's Repository link is `inline-flex` with `min-block-size: 44px` at 40rem and under (`arcade.css`, D-956): 72 x 17 became 72 x 44 at 375, both hit-test points return the link (base: the card body took them), "Notify me" still passes, the card grew by 16 px (the actions row), nothing changed at 1024 or 1280, no sideways scroll.
- SEC-AUD-446: closed. Admin > Arcade's Coming soon list shows each entry's tier as the candidates list's chip ("Original" / "Clean-room reimplementation"), with no new string; the release form's boxes are unchanged and the hit test equals the base in count and status.

## Proposed rows (the commander numbers from SEC-AUD-448; low)
1. Admin > Arcade at 375: the Release and Remove buttons of a Coming soon entry (and the candidates' Feature buttons) are 28 px tall (`size="sm"` in `ArcadeAdmin.tsx`), and Release's below point is taken by Remove. Same in base. Needs the 44 px phone size or a gap; not touched this wave.
2. At 1024 the release Select's above/below points are taken by its form (informational on a mouse page; same in base).
No threat-model row.

## Changelog sentence
Members on a phone: the Repository link on a Coming soon card is a full-size touch target. Staff: Admin > Arcade shows each Coming soon game's tier beside its title.

## Owner questions
None. Defaults chosen: the link rule sits at the sheet's own 40rem (not 720 px), as its other 44 px rule does; the chip uses the existing `arcade.tier.original` / `arcade.tier.reimplementation` labels, not the card's short "Clean-room" tag.

## Cleanup
Scratch database `sr_p101_arcade_ui` dropped (psql printed `DROP DATABASE`). Stack stopped by PID tree (server 33308, Vite 17480); ports 8783, 5783 and 9483 checked free. `p101-arcade-bin` deleted. No stash used or touched. Lock: `heavy.sh release arcade` printed `released` and `heavy.sh status` printed `free` (07:22); the final status is in my reply. No shell of mine is waiting on `acquire`.

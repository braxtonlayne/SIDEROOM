# Wave 101, lounge: D-958, the Invite dialog and the Scrapbook gallery move to the kit's `NativeSelect`; and the wave's one builder web build

2026-11-18. Clock: started 06:19; code and spec by 06:25 (commit 13e6d5a7); lock `lounge` 06:41 to 06:48 (one stack round: base, then after, in one run; two re-runs of the after phase inside the same round for a class fix and the spec's comparison rules); fix commit 78dcc36d (06:49). Web build lock 07:03 to 07:05 (taken early: the lock was free and my tree is final; build 07:03 to 07:04, 1 min 3 s). Handoff committed after; clean up below.

## Done
Files: `apps/web/src/lounge/InviteDialog.tsx`, `apps/web/src/lounge/scrapbook/Gallery.tsx`, `e2e/tests/p101-lounge-shots.spec.ts` (new), this handoff. No sheet, kit file, fragment or string touched. `apps/web/src/lounge/apps/CommandForm.tsx` **not reached: left raw** (below).

## The eight selects
Base = the files put back to the base's text from copies in my scratch folder (no stash), one run, one database, same sweep world; `git status` clean afterwards. Boxes are w x h in px.

| # | file | picks | result | class | base -> after, 1024 x 768 | base -> after, 375 x 812 touch | hit test (375) |
|---|---|---|---|---|---|---|---|
| 1 | InviteDialog | Invite to channel | moved | none (the `.lng-field select` rule in `lounge.css` still matches: the select is still inside `.lng-field`; its `height: 36px` loses to the kit's `min-block-size: 40px`) | 520 x 36 -> 520 x 40 | 303 x 36 -> 303 x 44 | see Invite below |
| 2 | InviteDialog | Expire after | moved | none | 254 x 36 -> 254 x 40 | 145.5 x 36 -> 145.5 x 44 | |
| 3 | InviteDialog | Max number of uses | moved | none | 254 x 36 -> 254 x 40 | 145.5 x 36 -> 145.5 x 44 | |
| 4 | Gallery | Channel filter | moved | `lng-scr-input` **kept** beside the kit's (its `min-inline-size: 8rem` gives the filter row its width: without it the select was 72 wide, base 128) | 128 x 28 -> 128 x 40 | 128 x 27 -> 128 x 44 | see Gallery below |
| 5 | Gallery | Tag filter | moved | kept, same reason | 128 x 28 -> 128 x 40 | 128 x 27 -> 128 x 44 | |
| 6 | Gallery | Member filter | moved | kept, same reason | 128 x 27 -> 128 x 40 | 128 x 27 -> 128 x 44 | |
| 7 | CommandForm | Subcommand levels | **not reached: left raw** | | | | |
| 8 | CommandForm | An option with choices / a user, role, channel picker | **not reached: left raw** | | | | |

Why 7 and 8 are not reached: the form shows only for a slash command that has subcommands or options with choices, which needs a registered application with such a command. The only bot helper in `e2e/support/bot.ts` (`startBot`) registers one command with no options, and the SDK route needs the developer-portal flow; no earlier spec makes one inside a round. Left raw rather than moved unrun.

The ShelfPanel's "Choose a shelf" select (not mine, `scrapbook/ShelfPanel.tsx`) is in the gallery page: raw, 32 px, unchanged base and after (it fails the 375 hit test in both: taker the "Storage shelf" section). Listed for whoever owns that file.

### Invite dialog (opened from the server menu, seeded world, no invite generated)
- 1024 Light and Dark: selects 36 -> 40 px (+4 each row; the dialog grows 431.2 -> 439.2 and, being centred, its top edge moves up 4). Every control below the selects moved by at most the summed growth (figures in `compare.json`: Expire/Max uses +4 allowed 4, the invite row and its buttons +8 allowed 12); Close and the first select are higher only by the re-centring. Widths unchanged. Footer buttons ("Copy link", the list's Copy/Revoke) moved by the summed growth.
- 1024 hit test (the helper's points are 21 px away, made for a 44 px phone control): 0 pass, 8 fail in base and after, identical, not a fault of this change.
- 375 Light: dialog 345 x 502.8 -> 345 x 518.8, selects 44 px, **hit test 5 pass / 3 fail (base: the three selects, taker the dialog's `div.lng-form`) -> 8 pass / 0 fail**. No sideways scroll (375/375), the last control (Revoke) is reachable in the dialog's scroller.
- Keyboard (expiry select, all three configs): `:focus-visible` after a key, ring `2px solid rgb(0, 104, 201)` offset 1px (Light) and `rgb(0, 168, 252)` (Dark); ArrowDown changes 604800 to 0 ("never"), ArrowUp puts it back. No invite was made.

### Scrapbook gallery (`/channels/:guild/scrapbook`)
The Scrapbook is operator-gated and per server: the spec switches `lounge.scrapbook` on (admin and the server's module) and off again in a `finally`.
- 1024 Light: the three filters 128 x 28 -> 128 x 40, same x, the row of date fields stretches (+5.5 px, inside the select's growth), the Member row below moves +11 (allowed 24). Hit test 0 pass / 6 fail / 1 skipped in base and after (1024 points).
- 375 Light: 128 x 27 -> 128 x 44; **hit test 1 pass / 6 fail -> 4 pass / 3 fail**: the three filter selects now pass; the three left are the "Choose a shelf" select and the two date inputs (taker the filter `form`), all there in the base and not mine. No sideways scroll.
- **Keyboard not exercised on the gallery**: the world has no kept entries, so each filter holds only "Any" (ArrowDown cannot change it). The spec asserts a change only when there is more than one option. `:focus-visible` matched. Next: a gallery with entries needs a shelf and a message archived by reactions; not made this round.

## Shots: 14 made (`visual-sweep-shots/p101-lounge`), all 14 opened
`invite-1024-lounge-light`, `invite-1024-lounge-dark`, `invite-375-lounge-light`, `invite-1024-focus-lounge-dark`, `gallery-1024-lounge-light`, `gallery-375-lounge-light` and the `base-` twins of all six. What I saw: the browser's own arrow and chrome gone, the kit chevron at the inline end in muted ink (Light and Dark), the kit field fill and border, the focus ring round "7 days" in Dark in both trees; the Invite dialog is 8 px taller and the Gallery's filters 12 px taller with the same widths; nothing touches or overlaps. Not shot (not asked): Command form (not reached).

## Who imports each file
- `InviteDialog.tsx`: `lounge/LoungeDialogs.tsx` and `settings/server/sections/Invites.tsx`. LoungeDialogs is in the lazy Lounge chunk (`LoungeRoot`), Invites in the lazy Server settings chunk. Not first paint.
- `scrapbook/Gallery.tsx`: `lounge/LoungeShell.tsx` (lazy Lounge chunk).
- `apps/CommandForm.tsx`: `lounge/Composer.tsx` (lazy Lounge chunk); not changed.

## Tests and checks (all after my last edit, 06:49; exit codes read)
- web `tsc --noEmit -p tsconfig.json`: exit 0 (06:50). eslint on `InviteDialog.tsx` and `Gallery.tsx` (the web package's own): exit 0. prettier --check on the three files: clean. e2e `npx tsc --noEmit -p tsconfig.json` from `e2e/`: exit 0 (06:51). `node scripts/i18n-audit.mjs --max 0`: exit 0. `node scripts/check-doc-links.mjs`: exit 0 (run before this handoff's last text edit; docs only).
- Single vitest files, unedited: `lounge/scrapbook/__tests__/scrapbook.test.tsx` 20 passed; `lounge/__tests__/serverFlows.test.tsx` and `shellFlows.test.tsx` (they open the Invite dialog) 70 passed; `lounge/apps/__tests__/CommandForm.attachment.test.tsx` 2 passed (06:24, CommandForm unchanged). None finds a select by a dropped class.
- The spec: base phase `problems: []`, after phase `problems: []` against the base's figures (spec rules: select block size not smaller, 44 at 375 except the ShelfPanel's, width not smaller, controls moved by no more than the select growth above or beside them and the dialog's re-centring, hit test not worse and equal in count). I did not watch the spec fail without the change except through its first comparison runs, which flagged the 72 px filter width (fixed by keeping `lng-scr-input`) and the centred dialog's re-centring (a rule of the spec, not a page fault).

## Web build (07:03 to 07:05, tree of commit 78dcc36d, the files unchanged since)
`npx pnpm@10.12.1 --filter @sideroom/web build`: exit 0. `node scripts/check-bundle-budget.mjs`: exit 0, first paint `app/app-GsdQ1Zsg.js` **218 351 B gzip, room 37 649 B** (before 218 363 B, room 37 637 B: 12 B less). `node scripts/check-entrance-isolation.mjs`: exit 0 ("every public surface is clean"). `node scripts/check-staff-delivery.mjs`: exit 0. A budget miss: none. This is my tree only (wave 101's other builders' files are not in it).

**Where the kit sheet went.** In wave 100 `NativeSelect`'s CSS (class fragment `_listbox_2n8dp_54`, 2 907 B) was in `app/ServerSettings-*.css`. Now that the Lounge (Invite dialog, Gallery) and Server settings both use it, the bundler moved it to a shared chunk: **`app/Channels-B5HwtEnV.css`** (its script is `Channels-BvZES_-W.js`), and no other built CSS file holds it (`ServerSettings-PUagnRPX.css` is 2 115 B without it). Importers of that chunk: `LoungeRoot-B0WGInkC.js` and `ServerSettings-DEdyisDg.js`, both lazy. First paint (`app-GsdQ1Zsg.js`) does not import it; it names `Channels-B5HwtEnV.css` once, in its dynamic-import dependency list (the preload map of lazy routes), so it is not fetched at first paint. No public page (`index.html`, `auth.html`, `invite.html`, `dist/entrance/`) references it. The chunk name "Channels" is the bundler's pick for the shared module; the commander's merged build (chan, mod and library add more adopters) may name or place it differently: re-check with a search for `_listbox_` in `dist/app/*.css`.

## The commander's command (from `e2e/`, on a stack with the sweep's world: API 8787, Vite 5787, metrics 9487, `SR_ADMIN_REQUIRE_REAUTH=0`, `DATABASE_URL` and `SR_CLI_BIN` set)
```
SR_E2E_BASE_URL=http://localhost:5787 SR_P101_LOUNGE_ON=1 \
SR_P101_LOUNGE_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-lounge \
npx playwright test tests/p101-lounge-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
After the merge there is no base: delete `figures-base.json` from the folder first (the spec then only records and asserts the after rules; `SR_P101_LOUNGE_PHASE=base` is for a tree where the files are raw). The spec switches the Scrapbook on and off through the API.

## Proposed rows and sentences
- Changelog sentence: "The Invite dialog's three lists and the Scrapbook gallery's three filters use the kit's `NativeSelect`: 44 px on a phone, and the Invite dialog's selects now pass the touch-target test there."
- Inventory: none new. Proposed note for the existing select row: ShelfPanel's "Choose a shelf" select and the gallery's two date inputs fail the 375 hit test (taker the filter form / the Storage shelf section) in base and after; `scrapbook/ShelfPanel.tsx` still has a raw `<select>`; `apps/CommandForm.tsx` (2 selects) is left raw.
- No threat-model row. No AGENTS.md sentence.

## Owner questions
- `lounge.css` has `.lng-field select` (height 36, padding, background): it still styles the Invite selects on top of the kit's. Their look is the kit's in the shots; if you want the old rule gone, it is the editor's sheet (not this wave).
- Should the Command form's two selects get a seeded app with a choice command in the sweep world so a later wave can move them?

## Cleanup
Stack stopped by PID tree after the round (ports 8787 / 5787 / 9487 checked free), database `sr_p101_lounge_ui` dropped, `p101-lounge-bin` deleted, `apps/web/dist` is ignored and not staged, no stash touched, `e2e/test-results` untracked and uncommitted. `heavy.sh status`: after the stack round `free`; after the build `free`; just before the reply: see the final report.

# Wave 101, chan: D-958, 14 raw selects of five Server settings sections move to the kit's `NativeSelect`

2026-11-18. Clock (real clock; the brief's T is 06:50): started 06:19, code and spec committed 06:25 (fef55b86), fix and spec tweaks 06:42 (ad27bea8), handoff commit at the end. Lock `chan`: round 1 06:36 to 06:41 (base and after, one database), round 2 06:56 to 07:01 (base and after again, after my own fix, see below), round 3 07:11 to 07:14 (after only, to prove the spec's last tolerance). Each round released; `heavy.sh status` read `free` after each. Files: `apps/web/src/settings/server/sections/{Channels,Members,ChannelTabs,MemberBulk,AuditLog}.tsx`, `e2e/tests/p101-chan-shots.spec.ts` (new), this handoff. No sheet, no fragment, no kit file, no Rust, no web build.

## What changed
All 14 `<select>` became `<NativeSelect>` (import from `@sideroom/ui`): same id, value, onChange, disabled, children. **`className="ss-select"` is dropped on all 14**: the kit part gives the 100 % width, 40 px (44 px at 720 px and under or on touch: the same width rule `.ss-select` had, so no loss), padding, border, radius and ring. Nothing finds any of them by that class: `git grep` of `ss-select` and of the ids in `e2e/` and the `__tests__` folders finds only `p84-guild-shots.spec.ts:234` (a generic list of card-like selectors) and `p85-guild-shots.spec.ts:384` (the toolbar rule's `:not(.ss-select)` on inputs); the `.ss-select` rules stay in `server-settings.css` for Admin and the other sections.

**A fault I made and caught in round 1:** my first `sed` also deleted `className="ss-select"` from three raw `<input>`s that carried it (`#ss-member-after`, `#ss-member-before` in Members, `#ss-forum-reaction` in the forum settings). Round 1's comparison showed the date inputs about 19 px shorter and the forum rows shifted 18 px. The class is back on those three inputs (they are not selects and stay as in the base); round 2 shows them equal. Nothing else in the files changed (the diff against the base is the 14 selects and the five import lines; D-951's code in `Channels.tsx` is untouched).

## The 14 selects (round 2: one database; base = the five files put back from copies in my scratch folder, then the edited files; `git status` showed only my spec edit afterwards)
Boxes in px (inline x block). "1024" = 1024 x 768 Lounge Light (Dark identical); "375" = 375 x 812 touch Lounge Light. Hit test: `openCollapsed` then `hitTestAll` over the section or dialog; counts base = after in every row.

| file | id | moved | 1024 base -> after | 375 base -> after | hit test 375 (pass / fail), base = after |
|---|---|---|---|---|---|
| Channels (editor, Overview tab) | `ss-slowmode` | yes | 241.3 x 40 -> 241.3 x 40 | 343 x 44 -> 343 x 44 | 81 / 0 |
| Channels (editor) | `ss-parent` (Category) | yes | 241.3 x 40 -> 241.3 x 40 | 343 x 44 -> 343 x 44 | same editor |
| Members | `ss-member-role` | yes | 176 x 40 -> 180 x 40 | 176 x 44 -> 180 x 44 | 15 / 1 (see below) |
| Members | `ss-member-sort` | yes | 202 x 40 -> 206 x 40 | 202 x 44 -> 206 x 44 | same section |
| Members, Ban dialog | `ss-ban-delete` | yes | 360 x 40 -> 360 x 40 | 303 x 44 -> 303 x 44 | 5 / 0 |
| Members, Timeout dialog | `ss-timeout-len` | yes | 360 x 40 -> 360 x 40 | 303 x 44 -> 303 x 44 | 4 / 0 |
| Channel tabs, text channel Features | `ss-thread-archive` | yes | 241.3 x 40 -> 241.3 x 40 | 343 x 44 -> 343 x 44 | 77 / 0 |
| Channel tabs | `ss-thread-slow` | yes | 241.3 x 40 -> 241.3 x 40 | 343 x 44 -> 343 x 44 | same editor |
| Channel tabs, forum Features | `ss-forum-sort` | yes | 241.3 x 40 -> 241.3 x 40 | 343 x 44 -> 343 x 44 | 80 / 0 |
| Channel tabs, forum | `ss-forum-layout` | yes | 241.3 x 40 -> 241.3 x 40 | 343 x 44 -> 343 x 44 | same editor |
| Member bulk, Add role dialog | `ss-bulk-role` | yes | 360 x 40 -> 360 x 40 | 303 x 44 -> 303 x 44 | 4 / 0 |
| Member bulk, Ban dialog | `ss-bulk-delete` | yes | 360 x 40 -> 360 x 40 | 303 x 44 -> 303 x 44 | 5 / 0 |
| Audit log | `ss-audit-user` | yes | 161 x 40 -> 165 x 40 | 161 x 44 -> 165 x 44 | 2 / 0 (filter row) |
| Audit log | `ss-audit-action` | yes | 518 x 40 -> 522 x 40 | 343 x 44 -> 343 x 44 | same row |

**Selects left raw: none.** All 14 were reached on the sweep world plus data the spec makes.

- Heights equal the base everywhere and are 44 at 375. Where the select is sized by its content (Members' two filters, Audit log's user filter, and the audit action filter at 1024) it is **4 px wider** than the base: the kit's end padding is 36 px for the chevron where `.ss-select` had 12. Nothing is narrower. In the two flex rows this hands 4 px to the neighbours (Members at 1024: the search input is 4 px narrower and the role filter starts 4 px earlier; Members at 375: the Prune button is 4 px further along). Those are the only moves of any control (round 2 printed exactly three "moved sideways" entries, all that 4 px; the spec now allows a sideways move up to the summed width growth of the target's selects, and round 3 ran clean on it). No control moved vertically (0 px against an allowed 0), no overlap between a select and another control in any config, no sideways page scroll (scrollWidth equals clientWidth in all 27 target runs).
- **Hit test at 1024:** the helper's points are 21 px from the centre and meant for 44 px phone controls; every 1024 control is 40 px or less there and fails in the base and after alike (counts identical in all 18 runs, takers identical; for instance the editor 0 pass / 80 fail in both). Not a fault of this change. **At 375 (touch) 8 of 9 targets have zero fails in both trees.** The one: Members at 375, the "Select all members on this page" checkbox fails in the base and after the same way: its centre is taken by a table cell (`td`, "Nov 18, 2026") and its upper point by the first row's "Select Juniper" checkbox. Existing, not from this change; proposed row below.
- **D-951 holds** (checked in both trees): at 375 after a pick of the text channel among 24 channels the editor heading's top is at 114 px of 812 (in view), the heading is the focused element, "Back to channels" is shown and passes the hit test (with sides), base and after the same; at 1024 the button is not rendered. D-951's code in `Channels.tsx` is untouched.
- **Keyboard**, one select per file (1024 Light; `:focus-visible` matched after a key; ring `2px solid rgb(0, 104, 201)`, offset 1px, base and after identical): Channels `#ss-slowmode` 0 -> 5 with ArrowDown and the unsaved-changes bar shown, back to 0 with ArrowUp (nothing saved); Members `#ss-member-sort` name -> newest -> name (the first row stayed "Juniper" in both trees: the two seeded members sort the same way, so this proves the value and the handler, not a reorder); Member bulk `#ss-bulk-delete` 0 -> 86400 -> 0; Channel tabs `#ss-forum-sort` 0 -> 1 -> 0 (nothing saved); Audit log `#ss-audit-action` "" -> 1 -> "" and the list refetched (section text length 5666 -> 1951, base 4050 -> 1951: the log is live). The text-channel thread selects save on change, so I did not drive them.
- **Themes:** Lounge Light and Lounge Dark at 1024 and Light at 375 for every target (27 runs per phase); `data-theme` read from the page each time; the account theme set by PATCH and localStorage and read back.
- **Data size (D-961):** the editor ran with 24 channels (the world's 6 plus `p101-chan-text`, `p101-chan-cat`, `p101-chan-forum` and 15 `p101-chan-fill-<n>`), the size where D-951's fault showed; the other targets ran on the same server. The spec removes all of its channels in a `finally` (names with its own prefix, D-942).
- **Audit log scope:** the entries below the filter row are a live table (the log grows with every run, and the spec's own channel creations add rows), so the spec's scope for that section is the filter row (`.ss-toolbar`), for boxes and for the hit test. In round 1 the whole-section control count differed 31 -> 53 for that reason (my own audit rows between the base and after runs); noted, not compared.
- **Round 3** (after only, against round 2's base figures from a different database) printed 12 "moved up" entries, all the member table's second row and the pager at 1024 (-8.5 and -17 px): "Joined via: Invite <code>" wraps differently with another random invite code. Data of the other database, not a select fault; the same-database comparison is round 2. Everything else in round 3 was clean (the three configs, the 14 selects, the hit tests, D-951, the sideways tolerance).

## Who imports my files
- `Members.tsx`, `ChannelTabs.tsx` (imported by `Channels.tsx`), `MemberBulk.tsx` (imported by `Members.tsx`), `AuditLog.tsx` and `Channels.tsx`: `ServerSettings.tsx` imports them, and `ServerSettings` is loaded with `lazy(() => import('../settings/server/ServerSettings'))` in `routes/settingsServer.tsx`: the Server settings chunk, not first paint, not a public page.
- `Channels.tsx` (the `ChannelEditor`) is also imported by `lounge/shell/EditChannelDialog.tsx`, which `lounge/LoungeDialogs.tsx` imports; `LoungeShell` imports that and `routes/LoungeRoot.tsx` is loaded with `lazyNamed(() => import('./LoungeRoot'))` in `routes/lounge.tsx`: the Lounge chunk, lazy. The kit sheet lands in the Lounge chunk too (lounge's build will show it). **The Lounge's edit-channel dialog is not measured by me**: it renders the same `ChannelEditor`, so its Slowmode and Category selects are the same kit part, but I did not open that dialog (the spec measures the Server settings page).
- `admin/AdminApp.tsx` imports its own `./sections/AuditLog`, not mine.

## Tests and checks (after my last edit: TSX files 06:40, spec 07:03)
- Web `npx pnpm@10.12.1 --filter @sideroom/web exec tsc --noEmit -p tsconfig.json`: exit 0 (07:15).
- eslint on the five TSX files (`pnpm --filter @sideroom/web exec eslint`): exit 0 (07:15). prettier `--check` on the five files and the spec: clean, exit 0 (07:14).
- e2e `npx tsc --noEmit -p tsconfig.json` from `e2e/`: exit 0 (07:15).
- `node scripts/i18n-audit.mjs --max 0`: exit 0, TOTAL 0 (07:15). `node scripts/check-doc-links.mjs`: ok (07:15).
- Single vitest files in `apps/web/src/settings/server/__tests__/` (unedited): `channels.test.tsx` 16 passed and `people.test.tsx` 17 passed (rerun 07:16 after the last TSX edit); at 06:30 also `p18channelModules` 4, `content` 14, `shell` 6, `featureSlots` 4, `viewAsMode` 5, all passed.
- The spec ran against the base text (rounds 1 and 2) and the base figures are `figures-base.json`; the base is the control, so there is no "failed without the change" test.

## Shots: 18 made, 18 opened (`visual-sweep-shots/p101-chan`)
`channels-editor-375-lounge-light`, `channels-editor-1024-lounge-light`, `channels-editor-1024-lounge-dark`, `channels-editor-focus-lounge-dark`, `members-375-lounge-light`, `members-1024-lounge-light`, `channel-tabs-375-lounge-light`, `audit-log-1024-lounge-light`, `member-bulk-375-lounge-light`, each with a `base-` twin. What I saw: the base shows the browser's own small arrow and text inset; the after shows the kit's chevron at the inline end in muted ink (Light and Dark), the text 4 px further in, the same boxes, borders and corners. The Category and Slowmode selects at 375 are full width and 44 px with the heading and Back button as before; the Ban dialog's select at 375 fills the dialog's width in both trees; Audit log's two filters keep their layout; the Members filter row at 375 is the same wrap with the two filters 4 px wider. The focus shot shows the blue ring round "Off" in Lounge Dark (the ring in Dark that wave 100 did not shoot), the same ring in the base. The forum's selects and the Timeout and Add role dialogs have figures but no shot (not on the list).

## The commander's command (from `e2e/`, on a stack with the sweep's world: API 8784, Vite 5784, metrics 9484, `SR_ADMIN_REQUIRE_REAUTH=0`, `DATABASE_URL` and `SR_CLI_BIN` set)
```
SR_E2E_BASE_URL=http://localhost:5784 SR_P101_CHAN_ON=1 \
SR_P101_CHAN_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p101-chan \
npx playwright test tests/p101-chan-shots.spec.ts --project=chromium --workers=1 --retries=0 --reporter=line
```
After the merge there is no base: the spec records and asserts the `after` rules only (select at least 44 px at 375, D-951, no sideways scroll, no overlap, no `not-reached`). Delete `figures-base.json` first if the folder holds a base from another tree. `SR_P101_CHAN_PHASE=base` is for a tree whose files still have raw selects. It needs the sweep's second member (Members and the dialogs) and takes about 2 minutes; it makes and removes its own channels (`p101-chan-*`). The commander's merged run is on a shared world, so a "moved" entry in the member table can come from data (round 3 above), not from a select.

## Docs
No doc states these selects' look; nothing changed.

## Proposed inventory rows
- Closes (D-958, chan part): the raw selects of Channels, Members, Channel tabs, Member bulk and Audit log (14), including the editor's Category select at 375.
- Proposed: "Server settings > Members at 375: the 'Select all members on this page' checkbox fails the hit test; its centre is taken by a table cell and its upper point by the first row's checkbox. Base and after identical; a wave 101 finding, not from the select move."
- Proposed (low): "Members' two date fields and the forum settings' reaction field still use `.ss-select` on raw inputs; they could take the kit's field."
- No threat-model row.

## Changelog sentence
"Server settings' Channels (the channel editor's Slowmode and Category), Members (filters, Ban and Timeout dialogs), Channel tabs, Member bulk and Audit log now use the kit's select, with a chevron, the kit's focus ring and a 44 px height on a phone."

## Owner questions
- None. Default taken: the selects that size to their content (Members' filters, Audit log's user filter) are 4 px wider than before because of the chevron's room; I left the kit's padding alone (the kit is not mine).

## Cleanup and lock
Stack stopped by PID tree after each round, ports 8784 / 5784 / 9484 free, `sr_p101_chan_ui` dropped, `p101-chan-bin` deleted, my scratch folder removed, no stash touched. The final `heavy.sh status` is in the reply.

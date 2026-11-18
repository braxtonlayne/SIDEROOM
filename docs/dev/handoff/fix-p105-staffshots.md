# fix/p105-staffshots (pod staff shots; D-1002, D-1011)

## Clock
Started 15:30. Spec compiled 15:36, base shoot 15:37-15:38, merged donate (dfd08405) and bridge (f49ac5cc) 15:38, build, after shoot 15:41. Reported ~15:50.

## Done
`e2e/tests/p105-staff-shots.spec.ts` (new): invoices (1280, 1024 L/D, 375), entitlements (1024 L/D, 375), grant dialog (1024), Audit log/Bans/Storage (1024), Conflict log (1024, 375), bridge map (1024 L/D, 375), plan link fieldset (1024, 375). Bridge answered by route fakes (status, features, discord-channels, conflicts, preflight, plan). No data made, nothing to remove. No invoice exists in the seeded world, so the table figures read "no table": the table, its first row and shade were NOT measured (the page shows the empty state). The plan's link fieldset IS reachable (fake plan with one "only here" and one "only on Discord" channel).

## Figures (base -> after), light 1024 unless noted
- Invoices State: 270x40 -> 274x40, type 14/400/none/normal (same in base: the raw select was already 14px), height 40 (44 at 375), now kit chevron. Purpose 145 -> 149 wide, x +4. Label text boxes and tops unchanged, label type unchanged (12/600/uppercase). Rows 136,204,300 unchanged. At 1280 the Search field moves x 771 -> 779 and narrows 397 -> 389 (Search is on the "must not change" list). Keyboard on State: ok. Hit tests equal.
- Entitlements Kind: 123x40 -> 127x40 (limit +36, height was already 40), gap label text to field 8 -> 8, toolbar height 40 -> 40, checkbox label x +4 only. 375 and dark pass. Keyboard ok.
- Grant dialog Kind label: base 16/400/none -> after 12/600/uppercase/0.48px/muted, equal to Account id, Days, Reason labels. Kind select 14px, none (the other dialog fields are 16px: visible in the shot, not a limit of the task).
- Audit log, Bans, Storage, Conflict log (Kind 14px/600, gap equal): no difference in any box or type.
- Bridge map selects (3): 12/600/none -> 14/400/none/normal/--ui-text; boxes (521.3x40 at 1024), label boxes, label text type, gap 0 unchanged. 1024 dark, 375 pass.
- Plan link fieldset (2): same, pass at 1024 and 375.

## FAULTS
1. donate, `donations.css` / `InvoicesSection.tsx`, the Invoices State and Purpose selects: the kit select is 4 px wider than the raw one (kit `padding-inline: 12px 36px` on the content-sized label). Figures: State 270 -> 274, Purpose 145 -> 149 and x +4, at 1280 the Search field x 771 -> 779 and width 397 -> 389; at 1024 and 375 the rows do not change (Search fills its own row). Cure asked: (b) a named fix, rows it may move: `.inv-field .inv-pick { padding-inline-end: 32px }` in `donations.css` (must bring State to 270 and Purpose to 145; moves only the chevron's room), OR the commander accepts +4 / -8 as the kit's own look (the task's "may change" says height and look, not width). Not fixed by me: no app file is mine.

## JOBS
0029 shoot base: submitted 1795034111, started +103 s, ended +69 s later, rc 0. 0039 build: waited 4 s, ran 21 s... (see files), rc 0. 0043 shoot after: waited ~54 s, ran 66 s, rc 1 (the 20 problem lines are all fault 1). Epoch values in cli-agents/run/jobs/<id>/.

## Shots (opened after job end, base twin beside after where noted)
invoices-1024-light: State and Purpose now with chevron, 4 px wider, same rows (keyboard focus ring on State from the test). invoices-1280-light: filters, search and its hint on one row, Search field a little narrower than base. invoices-1024-dark: kit selects dark, readable. invoices-375-light: each filter on its own row, 44 high. entitlements-1024-light/dark, 375: Kind label, kit select, "Only active" on one line, no crowding. grant-1024-light: all labels small caps equal, Kind select 14px. bridgemap-1024-light/dark, 375: three selects regular 14px under small-cap labels, fieldset layout as base. plan-1024-light: two selects regular under "Only here" and "Only on Discord". conflicts-1024-light: Show filter bold 14px as base, toolbar unchanged.

## Imports
Spec only; no app file changed.

## Checks
e2e tsc: rc 0 (15:46). prettier --check: pass. eslint: not run (pnpm not on PATH in the shell). Vitest: not run (no app file mine).

## Owner questions
None.

## PROCESSES
strays: none found

## Commander's command
bash C:/Users/braxt/Desktop/SIDEROOM_wt/cli-agents/job.sh submit integ shoot after --wt lanes --dist C:/Users/braxt/Desktop/SIDEROOM_wt/p105-integ/apps/web/dist --env SR_P105_STAFF_ON=1 --env SR_P105_STAFF_PHASE=after --env SR_P105_STAFF_OUT=C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p105-staff -- tests/p105-staff-shots.spec.ts

## FAULT 1 ended: cure holds (16:0x)
Donate's cure 2a860621 (`.inv-field .inv-pick { padding-inline-end: 32px }`) merged; build 0053 rc 0; after shoot 0057 rc 0, problems-after.json is `[]`. State 270, Purpose 145, Search x 771 / width 397 at 1280, as base. Shots invoices-1280-light and invoices-1024-light opened: selects keep the kit chevron, rows and Search field as base. strays: none found.

# fix/p106-kit handoff (D-1018, SEC-AUD-476)

## Clock
Started 16:30. Unit test + CSS commit 16:31. Base shoot done 16:36. After rounds: 0094 failed (my spec's kit detector missed the color(srgb) computed value), 0106 stale build (any commit changes the tree signature: HEAD is in it), 0135 failed (my spec compared the border under the focus ring, which is meant to change), 0149 passed 17:40 (problems: none). Runner was stopped for mount's cargo until about 16:54; queue waits were 10 to 15 minutes.

## Done
- `D-1018 kit field resting border: 65% faint mix, test`: three sheets, `border-color: color-mix(in srgb, var(--ui-border), var(--ui-text-faint) 65%)` after the old `border` line (Input.module.css:21, NativeSelect.module.css:35, Select.module.css:11, the `.control` rules only; the popup border in Select.module.css is untouched). `tokens-contrast.test.ts`: new describe "kit field resting border (D-1018)".
- `D-1018 kit shots spec`: `e2e/tests/p106-kit-shots.spec.ts`.

## N and its table (unit test, sRGB mix, worst case over all 7 themes in THEMES and grounds --ui-bg, --ui-bg-raised, --ui-bg-overlay)
45%: 2.33 (sideroom on raised) | 50%: 2.52 | 55%: 2.73 | 60%: 2.92 (lounge-light on raised) | **65%: 3.12 (lounge-light on raised)** | 70%: 3.34 | 75%: 3.55 | 80%: 3.79.
N = 65. Hover (`--ui-text-faint`) stays at least 1.25 times the resting ratio on every ground in every theme (asserted). The Modal draws `--ui-bg-overlay` (Modal.module.css:20), not raised, so it is included.

## FINDERS
`git grep -n "ui-border" -- e2e apps/web/src/**/__tests__`: only comments in p105-border-shots.spec.ts (retired) and p79-tools-shots.spec.ts. No CI spec or test reads a kit field's border colour. None to fix.

## First paint
Input.module.css lands in `entrance/auth-*.css` (the public sign-in page: Login.tsx uses TextInput) and in `app/app-*.css`; NativeSelect and Select in lazy chunks `app/NativeSelect-*.css`, `app/Select-*.css`. Gzip, base -> after: auth 1834 -> 1859 (+25 B); app 8392 -> 8411 (+19 B); NativeSelect 612 -> 638 (+26); Select 1093 -> 1118 (+25). `node scripts/check-entrance-isolation.mjs` on my build: passed (error 3 documents no script; static 6 files; every public surface clean).
(Note: the public sign-in page now carries the rule; it is CSS only, no script, no app code.)

## Answer
User settings theme picker (`settings/user/prefs.ts` THEME_CHOICES, Appearance.tsx): lounge-editorial, lounge-dark, lounge-light, lounge-ash, lounge-onyx, sideroom. It does NOT offer `sideroom-broadcast`: as designed, it is Live TV's section theme.

## Base figures
Base job 0084 reproduces the border report (e.g. lounge-light page 1.61/1.45, card 1.45; sideroom card 1.19).

## Not done
Nothing outstanding. The lounge-light Library shot (kit-library-lounge-light-1024.png) was taken but not opened by me; its figures are in the table (select 1.61/1.45 -> 3.48/3.13). Input/textarea of Settings overview are above the scroll in the 1024 settings shots (the page scrolls to the form), so I saw them only in the figures and in the dialog shots.

## JOBS (id, class, submitted -> ended, rc)
0084 shoot base 16:32->16:34 rc0 | 0085 build rc0 | 0094 shoot after rc1 (spec) | 0106 shoot after rc3 stale | 0120 build rc0 | 0135 shoot after rc1 (spec) | 0144 build rc0 | 0149 shoot after 17:39->17:40 rc0. The base job 0084 was run before the build; shots in the folder are from 0149 (after) and 0084 (base- prefix).

## Figures (job 0149, 52 kit fields, base twin for each; 0 problems)
May change: resting border colour. Border against outside / against field, base -> after, by theme (page; card; hover unchanged):
- lounge-light 1.61/1.45 -> 3.48/3.13 (card 1.45 -> 3.13); hover 5.76/5.19 equal
- lounge-dark 1.24/1.35 -> 3.13/3.42 (card 3.42); hover 4.81/5.24 equal
- lounge-ash 1.34/1.56 -> 3.31/3.85 (card 3.85); hover 4.98/5.78 equal
- lounge-onyx 1.32/1.25 -> 4.29/4.08 (card 4.08); hover 7.48/7.11 equal
- lounge-editorial 1.36/1.47 -> 3.69/4.00 (card 4.00); hover 5.88/6.36 equal
- sideroom 1.32/1.19 -> 3.51/3.15 (card 3.15); hover 5.68/5.10 equal
- Create Channel dialog: light 1.61/1.45 -> 3.48/3.13, dark 1.35 -> 3.42; Library select sideroom 1.32/1.19 -> 3.51/3.15, lounge-light 1.61/1.45 -> 3.48/3.13; 375 form same as light 1024.
Minimum over all 52 fields: 3.13 against outside, 3.13 against the field.
Must not change (asserted by the spec, all held): every kit field's box equal to base (x, y, w, h); hover border colour and ratios; focus ring (style, width, colour, ratio; the border under the ring follows the resting border, by design); text and placeholder ratios; a non-field --ui-border edge per page (a card section or panel: same colour and box in every theme). Sampled edge colours equal base, e.g. lounge-light card rgb(201, 204, 209). 4 pages had no sample (null equals null in the compare). A table line was not separately sampled.
Hover is still visible as a step: border 3.1-4.3 at rest, 4.8-7.5 on hover (asserted >= 1.25x in the unit test on all grounds).
Computed colour is serialised by Chrome as color(srgb ...) (e.g. lounge-light color(srgb 0.5257 0.54 0.5673) = #868a91). Anything that parses a field's border colour with an rgb()-only parser must accept color(srgb); e2e/support/contrast.ts already does.

## Shots (all opened after job 0149; base twins were not re-opened side by side except lounge-light)
- kit-lounge-light-1024: scrolled to the System messages card; the select now has a clear grey outline against the card, the fieldset outline is the same as before. Same product, same boxes (matches base twin pixel layout).
- kit-lounge-dark-1024: select outline clearly visible against the card; fieldset line unchanged.
- kit-lounge-ash-1024: same, visible.
- kit-lounge-onyx-1024: visible; strongest of the dark themes (4.1-4.3).
- kit-lounge-editorial-1024: visible, warm-grey outline, matches the palette.
- kit-sideroom-1024: visible, warm grey; no longer vanishing into the card.
- kit-dialog-light-1024: Channel name field has a defined outline on a white dialog; the radio rows (not kit fields) are unchanged tiles.
- kit-dialog-dark-1024: Channel name field clearly outlined in the dark dialog.
- kit-library-sideroom-1024: the Recently updated select is clearly outlined; the chip buttons and Grid/List (not kit fields) unchanged.
- kit-form-375-light: the System messages select outlined against its card; boxes as base.
Do the fields now read as fields: yes in every theme at 3:1+. Same product: yes; only the field outlines are a step firmer, hover still steps up.

## Who imports and which chunk
Input.module.css (TextInput, textarea): imported by TextInput, which routes/entrance/Login.tsx and the other entrance routes use, so the rule is in the public sign-in CSS (entrance/auth-*.css) and also in app/app-*.css. NativeSelect.module.css and Select.module.css: lazy chunks app/NativeSelect-*.css and app/Select-*.css. Gzip base -> after: auth 1834 -> 1859 (+25 B); app 8392 -> 8411 (+19 B); NativeSelect 612 -> 638; Select 1093 -> 1118. Entrance isolation check on my build: passes. Dependants of the three rules (D-990): every TextInput, textarea, NativeSelect, Select in the app (git grep -l "TextInput|NativeSelect" apps/web/src: 50 files); only the colour changed, no box property, so no layout row moves (spec confirms on 52 fields).

## Checks (my tree at last commit)
- vitest src/theme/tokens-contrast.test.ts (packages/ui): 79 passed, exit 0, 17:41.
- prettier --check on the 5 files: passed. node scripts/check-css-tokens.mjs: passed. node scripts/check-entrance-isolation.mjs: passed (on my build).
- e2e tsc --noEmit: clean. Web eslint on the spec: not run (the web package ignores files outside its base path). web tsc: not run (no .ts/.tsx app edit).

## Closing text for SEC-AUD-476
Closed by D-1018: a kit field's resting border is a 65% mix of --ui-border with --ui-text-faint (smallest 5-step N at which every theme reaches 3:1 on --ui-bg, --ui-bg-raised and the Modal's --ui-bg-overlay: worst 3.12 at 65, 2.92 at 60). Measured built, 52 fields, 3.13 to 4.29:1 against outside and field in all six main themes, page, card, dialog, Library select, phone. --ui-border, hover, focus, boxes unchanged.
Proposed new rows: none from this change. Not covered: app fields that are not the kit's (builder "fields" counts them).

## Changelog sentence
Form fields (text boxes, text areas and drop-downs) now have a firmer outline at rest in every theme, so you can see where a field begins and ends.

## Owner questions (Fable)
None blocking. Default taken: one mix for every theme, hover stays --ui-text-faint.

## PROCESSES
strays: none found (strays.sh p106-kit, 17:41).

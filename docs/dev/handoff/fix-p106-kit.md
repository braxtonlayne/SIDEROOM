# fix/p106-kit handoff (D-1018, SEC-AUD-476) - DRAFT at 16:55, after-round pending

## Clock
Started 16:30. Unit test + CSS commit 16:31. Base shoot job 0084 and build 0085 done by 16:35. After shoot 0094 queued from 16:35 (runner stopped for mount's cargo until about 16:54).

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

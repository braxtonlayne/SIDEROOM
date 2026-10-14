# Handoff: feat/p9-i18n-fmt (XC-I18N-002, XC-I18N-003, extraction follow-ups) - COMPLETE (pending owner review)

Worktree C:/Users/braxt/Desktop/SIDEROOM_wt/p9-i18n-fmt, branch feat/p9-i18n-fmt (base integration/phase9). Local commits only.
CARGO_TARGET_DIR=C:/Users/braxt/Desktop/SIDEROOM_wt/p9-i18n-fmt-target. No migrations.

## Done

- **Formatters.** `i18n/runtime.ts` gained `formatDateTime`, `setFormatLocale` (region override) and a `withHour12` rule so
  every time/date-time honours the 12/24 hour pref; all of them return `Invalid Date` instead of throwing. `settings/user/prefs.ts`
  feeds `prefs.locale`/`prefs.timeFormat` into the runtime (`syncFormatting`), so no screen reads prefs for formatting.
  About 45 call sites moved off `toLocale*String`/`new Intl.*` (admin, cinema, library, lounge, review, settings, support,
  developers, entrance server invite). `lounge/time.ts` is now a thin wrapper (keys `lounge.time.todayAt|yesterdayAt`).
  `support/api.ts` currency/sats use `formatNumber` (still `$1.00` in English). Left as is on purpose: the Language settings
  preview (`settings/user/sections/App.tsx`, previews the unsaved choice) and the time-zone validity probes in
  `admin/cinema/tv/ChannelEditor.tsx`.
- **Plurals.** `lounge/msg/format.ts` (poll time left), `cinema/player/model.ts` (`spokenClock`), `watchparty/sync.ts` are ICU plurals.
- **RTL.** 40 CSS files (apps/web, packages/ui, packages/markdown-react) now use logical margin/padding/border/text-align/float/
  radius/inset properties; inline styles for three physical props too. 15 `left`/`right` offsets stay physical (a `translateX(-50%)`
  centres the box, a keyframe slides it, or an off-screen skip link): see the ratchet in `i18n/format.test.tsx`.
  Pseudo-RTL smoke (`ar-XB`, `dir=rtl`, renders ShortcutSheet) and a CSS regression guard are in `apps/web/src/i18n/format.test.tsx`.
- **Extraction follow-ups** (fragments `i18n/messages/en/p9-1.ts` misc, `p9-2.ts` audit log, `p9-3.ts` permissions; 276 keys):
  cinema/tv/model.ts, cinema/player/model.ts (skip labels, spoken clock), settings/server/auditText.ts, settings/server/permissions.ts,
  lounge/shortcuts.ts (+ShortcutSheet), voice/quality.ts, library/progress.ts and continue.ts, support/api.ts (STATE_COPY, PURPOSE_COPY),
  watchparty/api.ts and sync.ts, settings/user/nav.ts, settings/user/whatsNew.ts, lounge/msg/format.ts. Tables keep their exported
  shape: label fields are getters that call `t()` when read. English output unchanged; every existing test passes.
- **Audit.** `scripts/i18n-audit.mjs` got `--ts` (plain `.ts` modules). `--ts` total 691 -> 537; the .tsx total stays 257 (all
  remaining hits are heuristic false positives such as generics, plus brand text and URL examples). `i18n-verify-diff.mjs`: MISSING 0.
- **Backend email (XC-I18N-003).** `crates/sr-api/src/routes/auth/mail_text.rs`: `MailKind` + per-locale `Catalog` (English only),
  `negotiate` (exact tag, primary language, default), per-kind English fallback, `{placeholder}` fill. `mail.rs` renders layout and
  escaping from it; every builder takes the account `locale` (reset, verify, change email, notice, email changed, password changed,
  2FA reset in sr-cli). Verification mail looks the account up for its locale and falls back to English. Tests in both modules.
  Wording is unchanged except the operator 2FA-reset mail, which now uses the standard notice layout (heading + one paragraph).

## Gates run

web `tsc --noEmit`, `eslint`, prettier check (apps/web, scripts, markdown-react), vitest 103 files / 1359 tests, ui 196 + markdown-react 44 tests,
build + `check-bundle-budget` (entrance 202.4 KB gzip vs 203.3 KB before: not enlarged). `cargo fmt`, `clippy -D warnings` (sr-api, sr-cli), tests
sr-api 407 + sr-cli 17.

## Not done / flags

- Entrance screens still hold literals (`EmailChangeCancel`, `LoginLink`, `PasswordReset`, `TwoFactor`, `ServerTemplate`: "Too many tries.
  Try again in N minute(s)" and "time(s)") with hand-built plurals; left alone (entrance chunk, unowned strings).
- Four-value shorthand margins/paddings (`margin: 0 0 0 8px`) were not rewritten; icons that imply direction are not mirrored.
- `packages/ui` CSS is compact and not Prettier-clean (it is in `.prettierignore`); I edited it with targeted replacements only.
- Account `locale` has no UI yet (the web Language pref is separate and stored in the settings blob); mails follow `accounts.locale`.
- Duration strings such as "1h 30m" in `cinema/tv/model.ts` are unit abbreviations and were not extracted.
- Key segments must match `[A-Za-z0-9]`, so permission and invoice-state keys are camelCased from their ids (`settings.permissions.viewChannel.label`).

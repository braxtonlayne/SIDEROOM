# UI string extraction conventions

Every user-visible English string in `apps/web/src` moves into the catalog, so it can be translated. The
catalog is a set of fragment files merged automatically (`apps/web/src/i18n/messages/en.ts`).

## Where your strings go

- Write ONLY your own fragment: `apps/web/src/i18n/messages/en/<fragment>.ts`.
- Format: `export default { 'key': 'English text', ... } satisfies Record<string, string>;` (plain object, no logic).
- Never edit `messages/en.ts`, `merge.ts` or another agent's fragment. A key defined in two fragments makes
  the merge throw (naming the key and both files), so keep your keys inside your own area/component.

## Key format

`<area>.<component>.<thing>`

- `area` is the top-level folder: `lounge`, `settings`, `library`, `cinema`, `admin`, `developers`,
  `watchparty`, `support`, `components`, `lib`, `review`, `pwa`.
- `component` is the component or module name in camelCase (`messageComposer`); `thing` is camelCase
  (`sendButton`, `emptyState`). Add more segments only when needed (`...rules.title`).
- One sentence per key. Do not split a sentence across keys or concatenate pieces; use ICU variables.

## DO extract

- JSX text.
- `aria-label`, `title`, `placeholder`, `alt`, `label`, `description`, `helpText` attributes.
- User-visible button, dialog, toast, notice and error text, empty states, `document.title`.

## DO NOT extract

- `className`, ids, test ids, `data-*` values, route paths, API field names, enum values.
- Code samples, curl/TypeScript snippets in docs pages.
- Console and log messages, error CODES (the machine value), CSS, units that are data.

## Variables and plurals (ICU)

- Variables: `'Linked as @{username}'` with `t('key', { username })`.
- Plurals: `'{n, plural, one {# item} other {# items}}'`.
- A literal brace is quoted `'{'`. Plain apostrophes are fine.

## Hard rule: output stays byte-identical

The rendered English must be exactly what it was, so existing tests keep passing. Keep the original quote
style and typography (curly apostrophes, ellipses, trailing punctuation, spaces). Do not trim or "improve" text.

## How to call

- Components: `import { useT } from '<rel>/i18n';` then `const t = useT();` inside the component, and
  `t('area.component.thing')`. Use string literals for keys (`catalog.test.ts` scans `t('...')` calls).
- Non-component modules: `import { t } from '<rel>/i18n';`.
- Module-level constants holding labels become keys resolved at render time (store the key, call `t(key)`
  where rendered), or become functions. Never call `t()` at module load (the locale is not known yet).
  Keys stored in tables are typed `MessageKey` (exported from `i18n`).

## Dates, numbers, clock times

Never call `toLocaleString`, `toLocaleDateString`, `toLocaleTimeString` or `new Intl.*` in a screen. Use the helpers
from `i18n`: `formatNumber` (also currency), `formatDate` (`{}` options give the numeric short form), `formatTime` and
`formatDateTime` (both follow the 12/24 hour preference), `formatRelative`. Pass a `Date` or epoch ms; wrap strings in
`new Date(...)`. Counts in sentences are ICU plurals (`{n, plural, one {# day} other {# days}}`), never
`n === 1 ? '' : 's'`. The Language prefs feed the helpers (`setFormatLocale`, `setHour12`, wired in
`settings/user/prefs.ts`), so no screen reads the prefs itself.

Tables of labels (`PERMISSIONS`, `QUALITY_LABEL`, `FILTERS`, ...) use getters that call `t()` when read, so the export
keeps its shape and follows the active language. Keys for table rows use camelCase segments
(`settings.permissions.viewChannel.label`).

## CSS direction

Use logical properties: `margin-inline-start/end`, `padding-inline-*`, `border-inline-*`, `text-align: start|end`,
`inset-inline-start/end`, `float: inline-start`. Keep `left`/`right` only where a `translateX` centres the box or a
keyframe slides it; `i18n/format.test.tsx` fails when new physical properties appear. Eyeball a screen with the
`ar-XB` pseudo locale.

## Never

Change types, exports or behaviour. If a string cannot be extracted without restructuring, leave it and
list it in your report under "Flags".

## Checking your work

- `node scripts/i18n-audit.mjs apps/web/src [--json out.json] [--ts]` lists files with remaining probable
  un-extracted strings (heuristic). `--ts` also scans plain `.ts` modules (label tables, formatters).
- `node scripts/i18n-verify-diff.mjs` compares text removed from your diff with the catalog and lists
  English text that disappeared or changed (heuristic, non-blocking).
- `cd apps/web && pnpm exec vitest run src/i18n` and `pnpm exec tsc --noEmit`, plus the tests of the folder
  you touched.

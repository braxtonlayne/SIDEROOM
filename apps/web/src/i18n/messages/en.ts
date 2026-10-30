import { en, registerFragments } from './store';

/**
 * English source catalog (XC-I18N-001). Keys are `<area>.<component>.<thing>`; values are ICU MessageFormat.
 *
 * The catalog is a set of FRAGMENT files, one per area or owner, in `./en/<fragment>.ts`. Each is
 * `export default { 'key': 'English text', ... } satisfies Record<string, string>;` and this file merges them
 * all at build time (`import.meta.glob`). Adding a fragment needs no edit here. Defining the same key in
 * two fragments throws at load, naming the key and both files.
 *
 * Rules for contributors:
 * - Edit only your own fragment (`messages/en/<fragment>.ts`); never another owner's.
 * - Use the key in code as a literal: `t('entrance.login.title')`. `catalog.test.ts` scans the source for
 *   `t('...')` calls and fails on a key that is not defined in any fragment.
 * - Plurals and numbers go through ICU (`{n, plural, one {# minute} other {# minutes}}`), never string
 *   concatenation. A plain apostrophe is fine; quote a literal brace as `'{'`.
 * - Keep one sentence per key. Translators see only the string, so add a comment above when the context
 *   is not obvious.
 * - Other locales live in `./<code>.ts` (typed `Catalog`) and are registered in `../runtime.ts`; any missing
 *   key falls back to this catalog.
 * - See docs/dev/i18n-extraction.md for what to extract and how.
 */
// Only the fragments the first paint needs are bundled with the app; `./en.rest.ts` loads the others.
const core = import.meta.glob<Record<string, string>>('./en/{entrance,a11y,pwa,p34-prf-profiles-core}.ts', {
  eager: true,
  import: 'default',
});
registerFragments(core);

export { en };

/** Keys are plain strings: the catalog is assembled from fragments, so there is no static key union. */
export type MessageKey = string;

import { registerFragments } from './store';

/**
 * Every English fragment except the first-paint ones registered by './en.ts'. The app loads this module
 * once, as its own chunk, before it renders (`loadFullCatalog` in '../runtime.ts'); tests import it in
 * `test/setup.ts`. Adding a fragment needs no edit here.
 */
const rest = import.meta.glob<Record<string, string>>(
  ['./en/*.ts', '!./en/entrance.ts', '!./en/a11y.ts', '!./en/pwa.ts'],
  { eager: true, import: 'default' },
);
registerFragments(rest);

export const restLoaded = true;

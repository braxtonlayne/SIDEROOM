import { markRestLoaded, registerFragments } from './store';

/**
 * Every English fragment except the first-paint ones registered by './en.ts'. The app loads this module
 * once, as its own chunk, in parallel with the first paint (`loadFullCatalog` in '../runtime.ts'); non-entrance routes wait for it (`CatalogGate`); tests import it in
 * `test/setup.ts`. Adding a fragment needs no edit here.
 */
const rest = import.meta.glob<Record<string, string>>(
  ['./en/*.ts', '!./en/entrance.ts', '!./en/a11y.ts', '!./en/pwa.ts', '!./en/p34-prf-profiles-core.ts'],
  { eager: true, import: 'default' },
);
registerFragments(rest);
markRestLoaded();

export const restLoaded = true;

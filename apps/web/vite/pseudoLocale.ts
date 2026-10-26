import type { Plugin } from 'vite';
import { buildPseudoCatalog } from '../../../scripts/pseudo-locale.mjs';

const ID = 'virtual:sr-pseudo-locale';
const RESOLVED = `\0${ID}`;

/**
 * RDY-I18N-002: serves the en-XA pseudo-locale catalog as the virtual module `virtual:sr-pseudo-locale`,
 * generated from the English fragments by `scripts/pseudo-locale.mjs`. It carries the catalog on the dev
 * server and in a build made with `SR_PSEUDO_LOCALE=1`; any other build gets an empty module, and the runtime
 * (`src/i18n/runtime.ts`) never imports it there, so a production bundle holds no pseudo text.
 * The catalog is read when it is first requested; restart the dev server after adding a fragment.
 */
export function pseudoLocalePlugin(): Plugin {
  let enabled = false;
  return {
    name: 'sideroom-pseudo-locale',
    configResolved(cfg) {
      enabled = cfg.command === 'serve' || process.env.SR_PSEUDO_LOCALE === '1';
    },
    resolveId(id) {
      return id === ID ? RESOLVED : undefined;
    },
    load(id) {
      if (id !== RESOLVED) return undefined;
      if (!enabled) return 'export default null;';
      return `export default ${JSON.stringify(buildPseudoCatalog())};`;
    },
  };
}

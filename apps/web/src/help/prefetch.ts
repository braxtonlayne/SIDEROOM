import { loadFullCatalog } from '../i18n/runtime';

/**
 * Warms what `/help` needs: the full message catalog and the What's new chunk. Called on pointer-enter or focus of
 * a help link, so the page is usually ready by the click. Both loaders are idempotent and failures are ignored.
 */
export function prefetchHelp(): void {
  void loadFullCatalog().catch(() => undefined);
  void import('./WhatsNew').catch(() => undefined);
}

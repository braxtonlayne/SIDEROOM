import { markRestLoaded } from '../../i18n/messages/store';

/**
 * Stands in for `i18n/messages/en.rest` in the public build (vite.public.config.ts): a public surface renders
 * from the core fragments (entrance, a11y, pwa) alone, so "the rest of the catalog" is already complete.
 * The app's strings are never in a public bundle.
 */
markRestLoaded();
export const restLoaded = true;

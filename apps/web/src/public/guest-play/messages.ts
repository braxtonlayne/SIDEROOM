import runner from '../../i18n/messages/en/p43-guest-runner-2';
import { registerFragments } from '../../i18n/messages/store';

/**
 * The runner page's strings. Registered by this surface alone, never by the shared first-paint catalog, so the
 * landing, sign-in and invite pages carry none of them (D-385). The key matches the one the app's `en.rest` glob
 * uses for the same file, so the catalog tests see one owner.
 */
registerFragments({ './en/p43-guest-runner-2.ts': runner });

import core from '../../i18n/messages/en/p34-prf-profiles-core';
import { registerFragments } from '../../i18n/messages/store';

/**
 * The strings of the block renderer and the public page. They are registered by this surface alone, never by the
 * shared first-paint catalog, so the landing, sign-in and invite pages carry none of them (D-385). The key matches
 * the one the app's `en.rest` glob uses for the same file, so the catalog tests see one owner.
 */
registerFragments({ './en/p34-prf-profiles-core.ts': core });

/**
 * SEC-AUD-039 (D-570, audit theme 10): what a session leaves on this device is removed when it ends, so the next
 * person on a shared browser finds none of it. Runs on sign-out (both buttons) and when the app learns that its
 * session is gone (expired or revoked elsewhere) before it leaves for the sign-in page.
 *
 * - the signed-in app files and shell the service worker keeps (`sr-app-*`, SEC-ENT-002);
 * - files shared into SIDE ROOM from another app and not yet attached (IndexedDB `sr-share`);
 * - the Library's offline chapters (Cache Storage `sr-library-offline-v1` and its registry, LIB-RDR-015);
 * - the Notes key this browser holds (IndexedDB `sr-tools-notes`, SEC-AUD-136; every account's, the key database is
 *   deleted whole), and an open Notes store is locked. `Clear-Site-Data` removes it too, but some browsers ignore it.
 *
 * Never rejects: a part that cannot be removed (storage blocked) does not stop the others or the sign-out.
 */
import { clearOfflineCache } from '../library/reader/offlineCache';
import { purgeNotesKeys } from './notesKeys';
import { purgeAppCaches } from './purge';
import { clearSharedFiles } from './shareFiles';

/**
 * Imported statically (a small module): a session that ended elsewhere gets 401 for every /app/ file, so a chunk
 * loaded on demand at that moment would never arrive and the offline chapters would stay.
 */
const clearLibraryOffline = () => clearOfflineCache();

export async function purgeOnSignOut(): Promise<void> {
  await Promise.all([
    purgeNotesKeys(),
    purgeAppCaches(),
    clearSharedFiles(),
    clearLibraryOffline().catch(() => undefined),
  ]);
}

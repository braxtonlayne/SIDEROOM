/**
 * SEC-AUD-039 (D-570, audit theme 10): what a session leaves on this device is removed when it ends, so the next
 * person on a shared browser finds none of it. Runs on sign-out (both buttons) and when the app learns that its
 * session is gone (expired or revoked elsewhere) before it leaves for the sign-in page.
 *
 * - the signed-in app files and shell the service worker keeps (`sr-app-*`, SEC-ENT-002);
 * - files shared into SIDE ROOM from another app and not yet attached (IndexedDB `sr-share`);
 * - the Library's offline chapters (Cache Storage `sr-library-offline-v1` and its registry, LIB-RDR-015).
 *
 * Never rejects: a part that cannot be removed (storage blocked) does not stop the others or the sign-out.
 */
import { purgeAppCaches } from './purge';
import { clearSharedFiles } from './shareFiles';

/** The Library module is loaded on demand so the sign-out path does not pull the reader into its chunk. */
const clearLibraryOffline = () =>
  import('../library/reader/offlineCache').then(
    (m) => m.clearOfflineCache(),
    () => undefined,
  );

export async function purgeOnSignOut(): Promise<void> {
  await Promise.all([
    purgeAppCaches(),
    clearSharedFiles(),
    clearLibraryOffline().catch(() => undefined),
  ]);
}

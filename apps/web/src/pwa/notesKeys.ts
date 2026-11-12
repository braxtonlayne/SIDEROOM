/**
 * SEC-AUD-136 (wave 61): removes the Notes key this browser holds. Notes keeps a non-extractable `CryptoKey`
 * (usable by this origin without the passphrase) and its passphrase-wrapped bundle in IndexedDB
 * (`tools/notes/crypto.ts`, `indexedDbKeys`); sign-out must not leave them behind on a shared computer.
 *
 * A tiny module on its own: `signOutPurge` is in the app's first load and must not import the Notes store. Keep
 * the database and store names in sync with `tools/notes/crypto.ts`. Never rejects; gives up after a moment when
 * storage is blocked, so a sign-out never waits on it.
 */
const DB = 'sr-tools-notes';
const STORE = 'keys';
const GIVE_UP_MS = 2000;

/** Asks open Notes stores (loaded only when Notes was used) to forget their in-memory key. */
export const NOTES_PURGE_EVENT = 'sr:notes-purge';

function removeRecords(userId: string | null): Promise<void> {
  return new Promise<void>((resolve) => {
    try {
      const req = indexedDB.open(DB, 1);
      // Notes never ran in this browser: do not create an empty database for it (Notes would then find no store).
      req.onupgradeneeded = () => req.transaction?.abort();
      req.onerror = () => resolve();
      req.onblocked = () => resolve();
      req.onsuccess = () => {
        const db = req.result;
        try {
          if (!db.objectStoreNames.contains(STORE)) {
            db.close();
            return resolve();
          }
          const tx = db.transaction(STORE, 'readwrite');
          const s = tx.objectStore(STORE);
          if (userId) {
            s.delete(`key:${userId}`);
            s.delete(`bundle:${userId}`);
          } else s.clear();
          tx.oncomplete =
            tx.onerror =
            tx.onabort =
              () => {
                db.close();
                resolve();
              };
        } catch {
          db.close();
          resolve();
        }
      };
    } catch {
      resolve();
    }
  });
}

/**
 * Removes the Notes key and bundle of `userId` (every account's when the account is not known, as when the
 * session ended elsewhere) and locks an open Notes store.
 */
export async function purgeNotesKeys(userId?: string | null): Promise<void> {
  try {
    window.dispatchEvent(new Event(NOTES_PURGE_EVENT));
  } catch {
    /* no window */
  }
  await Promise.race([
    removeRecords(userId ?? null),
    new Promise<void>((resolve) => setTimeout(resolve, GIVE_UP_MS)),
  ]);
}

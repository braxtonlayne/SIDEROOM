/**
 * SEC-AUD-136 (wave 61): removes the Notes key this browser holds. Notes keeps a non-extractable `CryptoKey`
 * (usable by this origin without the passphrase) and its passphrase-wrapped bundle in IndexedDB database
 * `sr-tools-notes` (`tools/notes/crypto.ts`); sign-out must not leave them behind on a shared computer.
 *
 * Tiny and on its own: `signOutPurge` is in the app's first load (its bundle budget has almost no room) and must
 * not import the Notes store. Keep the database name in sync with `tools/notes/crypto.ts`. Never rejects.
 */
export const NOTES_PURGE_EVENT = 'sr:notes-purge';

/** Locks an open Notes store (it listens only once Notes was used) and deletes the whole key database. */
export const purgeNotesKeys = (): Promise<void> =>
  new Promise((done) => {
    try {
      window.dispatchEvent(new Event(NOTES_PURGE_EVENT));
      const r = indexedDB.deleteDatabase('sr-tools-notes');
      r.onsuccess = r.onerror = r.onblocked = () => done();
      setTimeout(done, 2000); // storage that never answers must not hold the sign-out
    } catch {
      done();
    }
  });

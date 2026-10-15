/**
 * Share Target file hand-off (G-18, XC-PWA-006). The manifest declares a POST multipart share target; the
 * service worker (public/sw.js) cannot hand files to a page it has not opened yet, so it writes them to
 * IndexedDB (database `sr-share`, store `files`) and redirects to `/share?files=1`. This module reads and
 * clears that store. Keep the database and store names in sync with sw.js.
 */
const DB = 'sr-share';
const STORE = 'files';

function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    if (typeof indexedDB === 'undefined') return reject(new Error('indexedDB unavailable'));
    const req = indexedDB.open(DB, 1);
    req.onupgradeneeded = () => {
      if (!req.result.objectStoreNames.contains(STORE)) req.result.createObjectStore(STORE);
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error ?? new Error('indexedDB open failed'));
  });
}

function done(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error ?? new Error('indexedDB failed'));
    tx.onabort = () => reject(tx.error ?? new Error('indexedDB aborted'));
  });
}

/** Files the service worker stored, in the order they were shared. Empty when none or storage is unavailable. */
export async function readSharedFiles(): Promise<File[]> {
  try {
    const db = await open();
    const tx = db.transaction(STORE, 'readonly');
    const req = tx.objectStore(STORE).getAll();
    await done(tx);
    db.close();
    return (req.result as unknown[]).filter((f): f is File => f instanceof Blob) as File[];
  } catch {
    return [];
  }
}

/** Forgets the stored files once they are queued in a composer (or the user leaves the page). */
export async function clearSharedFiles(): Promise<void> {
  try {
    const db = await open();
    const tx = db.transaction(STORE, 'readwrite');
    tx.objectStore(STORE).clear();
    await done(tx);
    db.close();
  } catch {
    /* nothing to clear */
  }
}

/** Test helper: stores files the way sw.js does. */
export async function stashSharedFilesForTest(files: File[]): Promise<void> {
  const db = await open();
  const tx = db.transaction(STORE, 'readwrite');
  const store = tx.objectStore(STORE);
  files.forEach((f, i) => store.put(f, i));
  await done(tx);
  db.close();
}

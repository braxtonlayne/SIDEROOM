// The `storage` capability's backing store (ARC-SBX-001). A sandboxed Activity has no `localStorage` of its own
// (its origin is opaque), so the host page keeps the values under one key per (member, application) in the
// app's own storage. Browser storage that throws or is full degrades to a store that lasts for the page.
import type { ActivityStorage } from './host';

const PREFIX = 'sr.act.store.';

/** The storage key for one member's values of one application. */
export function storageKeyFor(userId: string | null, applicationId: string): string {
  return `${PREFIX}${userId ?? 'anon'}.${applicationId}`;
}

/** A store over `backing` (the page's `localStorage` by default) that reads the whole record once. */
export function createBridgeStorage(
  userId: string | null,
  applicationId: string,
  backing: Pick<Storage, 'getItem' | 'setItem' | 'removeItem'> | null = safeLocalStorage(),
): ActivityStorage {
  const key = storageKeyFor(userId, applicationId);
  let mem: Record<string, string> = {};
  try {
    const raw = backing?.getItem(key);
    const parsed: unknown = raw ? JSON.parse(raw) : {};
    if (typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)) {
      for (const [k, v] of Object.entries(parsed)) if (typeof v === 'string') mem[k] = v;
    }
  } catch {
    mem = {};
  }
  const persist = () => {
    try {
      if (Object.keys(mem).length) backing?.setItem(key, JSON.stringify(mem));
      else backing?.removeItem(key);
    } catch {
      // Blocked or full storage: the values last for this page.
    }
  };
  return {
    all: () => ({ ...mem }),
    set(k, v) {
      mem[k] = v;
      persist();
    },
    remove(k) {
      delete mem[k];
      persist();
    },
  };
}

function safeLocalStorage(): Storage | null {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

/** Download `bytes` as `name` from the host page (the sandboxed frame cannot). */
export function downloadBytes(name: string, mime: string, bytes: Uint8Array): void {
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type: mime }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.rel = 'noopener';
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

/**
 * Logout purge (SEC-ENT-002, entrance-isolation.md section 4). The sign-out call `await`s `purgeAppCaches()`
 * before it posts to the API: the service worker deletes every `sr-app-*` cache and answers `PURGED`. Where
 * the worker is missing or silent (first load, a browser that suspended it) the page deletes the caches itself,
 * so the result is the same either way. `Clear-Site-Data` from the server is the second layer, not the first.
 */

const APP_CACHE_PREFIX = 'sr-app-';
const REPLY_TIMEOUT_MS = 1500;

async function deleteAppCaches(): Promise<void> {
  if (typeof caches === 'undefined') return;
  try {
    const keys = await caches.keys();
    await Promise.all(
      keys.filter((k) => k.startsWith(APP_CACHE_PREFIX)).map((k) => caches.delete(k)),
    );
  } catch {
    /* nothing to purge, or storage is blocked */
  }
}

/** Ask the controlling worker to purge and wait for its `PURGED`, then make sure from the page. Never rejects. */
export async function purgeAppCaches(timeoutMs = REPLY_TIMEOUT_MS): Promise<void> {
  const controller =
    typeof navigator !== 'undefined' && 'serviceWorker' in navigator
      ? navigator.serviceWorker.controller
      : null;
  if (controller) {
    await new Promise<void>((resolve) => {
      const channel = new MessageChannel();
      const timer = setTimeout(resolve, timeoutMs);
      channel.port1.onmessage = (e: MessageEvent<{ type?: string }>) => {
        if (e.data?.type === 'PURGED') {
          clearTimeout(timer);
          resolve();
        }
      };
      try {
        controller.postMessage({ type: 'PURGE_APP' }, [channel.port2]);
      } catch {
        clearTimeout(timer);
        resolve();
      }
    });
  }
  await deleteAppCaches();
}

// Page image warm-up (LIB-RDR-007). `preloadImage` fetches and decodes a URL off the main thread
// (`HTMLImageElement.decode()`), remembering the promise so the same URL is never requested twice; the
// <img> that later shows it then paints from the decoded cache without a flash.
const warm = new Map<string, Promise<void>>();
const MAX_WARM = 64;

export function preloadImage(url: string): Promise<void> {
  const hit = warm.get(url);
  if (hit) return hit;
  if (typeof Image === 'undefined') return Promise.resolve();
  const p = new Promise<void>((resolve) => {
    const img = new Image();
    img.decoding = 'async';
    img.src = url;
    if (typeof img.decode === 'function') img.decode().then(resolve, resolve);
    else {
      img.onload = () => resolve();
      img.onerror = () => resolve();
    }
  });
  warm.set(url, p);
  // Bound the memo (insertion order = age); the browser's HTTP cache keeps the bytes regardless.
  if (warm.size > MAX_WARM) warm.delete(warm.keys().next().value as string);
  return p;
}

export const isWarm = (url: string) => warm.has(url);
export function clearWarm() {
  warm.clear();
}

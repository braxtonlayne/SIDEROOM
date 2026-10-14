import { useEffect, useState } from 'react';
import type { Animation } from '../../settings/user/prefs';

/** First frame of an image as a PNG data URL, or null when the browser will not let the page read it. */
function firstFrame(url: string): Promise<string | null> {
  return new Promise((resolve) => {
    const img = new Image();
    img.crossOrigin = 'anonymous';
    img.onload = () => {
      try {
        const canvas = document.createElement('canvas');
        canvas.width = img.naturalWidth || 1;
        canvas.height = img.naturalHeight || 1;
        const ctx = canvas.getContext('2d');
        if (!ctx) return resolve(null);
        ctx.drawImage(img, 0, 0);
        resolve(canvas.toDataURL('image/png'));
      } catch {
        resolve(null);
      }
    };
    img.onerror = () => resolve(null);
    img.src = url;
  });
}

/**
 * Animation setting for stickers and emoji (User Settings > Accessibility): `always` keeps the animated image,
 * `never` shows its first frame, `hover` shows the first frame until `active` (pointer over or focus).
 * If the first frame cannot be read (cross-origin image without CORS headers) the animated image is used.
 */
export function useAnimatedSrc(
  url: string | null,
  mode: Animation,
  active: boolean,
): string | null {
  const [frame, setFrame] = useState<string | null>(null);
  const wantsFrame = mode !== 'always' && !!url;
  useEffect(() => {
    setFrame(null);
    if (!wantsFrame || !url) return;
    let live = true;
    void firstFrame(url).then((f) => live && setFrame(f));
    return () => {
      live = false;
    };
  }, [url, wantsFrame]);
  if (!url || mode === 'always') return url;
  if (mode === 'hover' && active) return url;
  return frame ?? url;
}

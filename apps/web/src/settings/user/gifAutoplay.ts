import { prefersReducedMotion } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useSavedPrefs } from './prefs';

/** Whether the document has focus right now (a hidden tab or another window does not). */
const hasFocus = () =>
  typeof document === 'undefined'
    ? true
    : document.visibilityState !== 'hidden' && document.hasFocus();

/** Tracks window focus; GIFs only autoplay while the client is focused. */
export function useClientFocused(): boolean {
  const [focused, setFocused] = useState(hasFocus);
  useEffect(() => {
    const update = () => setFocused(hasFocus());
    window.addEventListener('focus', update);
    window.addEventListener('blur', update);
    document.addEventListener('visibilitychange', update);
    update();
    return () => {
      window.removeEventListener('focus', update);
      window.removeEventListener('blur', update);
      document.removeEventListener('visibilitychange', update);
    };
  }, []);
  return focused;
}

/** "Reduce motion" from Settings > Accessibility: on, or left on system and the OS asks for it. */
export function useReducedMotionPref(): boolean {
  const mode = useSavedPrefs().reducedMotion;
  return mode === 'on' || (mode === 'system' && prefersReducedMotion());
}

/**
 * RDY-PAR-006: should an animated GIF play on its own right now? Needs the preference on, the client focused,
 * and reduced motion off. When false, show the first frame and play on hover or focus (`useAnimatedSrc`).
 */
export function useGifAutoplay(): boolean {
  const on = useSavedPrefs().gifAutoplay;
  const focused = useClientFocused();
  const reduced = useReducedMotionPref();
  return on && focused && !reduced;
}

/** True for an image that can animate as a GIF. */
export const isGif = (contentType: string | undefined, nameOrUrl?: string): boolean =>
  (contentType ?? '').toLowerCase() === 'image/gif' || /\.gif(\?|#|$)/i.test(nameOrUrl ?? '');

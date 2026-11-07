import type { MessageKey } from '../i18n';

/**
 * A member action that failed after the menu or button that started it has gone (SEC-AUD-036): delete, pin,
 * vote, block, mute and the like run from plain functions with no component to show an error in. They report
 * here, and `ActionErrorToasts` (mounted once in the lounge shell) shows the existing danger toast.
 */
type Listener = (key: MessageKey) => void;
const listeners = new Set<Listener>();

export function reportActionError(key: MessageKey): void {
  for (const l of listeners) l(key);
}

export function onActionError(l: Listener): () => void {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

/** `promise.catch` handler that reports one failure key and swallows the rejection (it has been shown). */
export function failWith(key: MessageKey): () => void {
  return () => reportActionError(key);
}

import { useSyncExternalStore } from 'react';

/**
 * Tiny external store for PWA state (XC-PWA-002/003): "an update is waiting" and "the browser offers to
 * install". Module level so the `beforeinstallprompt` event, which fires once and early, is never missed.
 */

interface BeforeInstallPromptEvent extends Event {
  prompt(): Promise<void>;
  userChoice: Promise<{ outcome: 'accepted' | 'dismissed' }>;
}

export interface PwaState {
  /** A new service worker is installed and waiting. */
  updateReady: boolean;
  /** Chromium offered an install prompt we are holding. */
  canInstall: boolean;
  /** Running as an installed app already. */
  installed: boolean;
  /** The Lounge notices bar is mounted and shows the update notice itself (G-49). */
  noticesBar: boolean;
  /** The member asked for the update while music plays: it waits for the track boundary (MUS-PLY-001). */
  updateDeferred: boolean;
}

let state: PwaState = {
  updateReady: false,
  canInstall: false,
  installed: isStandalone(),
  noticesBar: false,
  updateDeferred: false,
};
let updateGuard: (() => boolean) | null = null;
let deferred: BeforeInstallPromptEvent | null = null;
let applyUpdate: (() => void) | null = null;
const listeners = new Set<() => void>();

function set(patch: Partial<PwaState>) {
  state = { ...state, ...patch };
  for (const l of listeners) l();
}

export function isStandalone(): boolean {
  if (typeof window === 'undefined') return false;
  const nav = navigator as Navigator & { standalone?: boolean };
  return !!nav.standalone || !!window.matchMedia?.('(display-mode: standalone)').matches;
}

/** iPhone/iPad Safari has no install prompt; it needs the Share sheet instructions. */
export function isIosSafari(
  ua: string = navigator.userAgent,
  touchPoints = navigator.maxTouchPoints,
): boolean {
  const ios = /iPad|iPhone|iPod/.test(ua) || (/Macintosh/.test(ua) && touchPoints > 1);
  const webkitOnly = /WebKit/.test(ua) && !/CriOS|FxiOS|EdgiOS|OPiOS/.test(ua);
  return ios && webkitOnly;
}

if (typeof window !== 'undefined') {
  window.addEventListener('beforeinstallprompt', (e) => {
    e.preventDefault();
    deferred = e as BeforeInstallPromptEvent;
    set({ canInstall: true });
  });
  window.addEventListener('appinstalled', () => {
    deferred = null;
    set({ canInstall: false, installed: true });
  });
}

export function setUpdateReady(apply: (() => void) | null) {
  applyUpdate = apply;
  set({ updateReady: apply !== null });
}

/** The Lounge notices bar calls this on mount (true) and unmount (false) so the toast does not repeat it. */
export function setNoticesBarMounted(mounted: boolean) {
  set({ noticesBar: mounted });
}

/**
 * Let a feature that must not be cut off (the floating player, MUS-PLY-001) hold the update reload back: while
 * `guard()` is true, `applyPendingUpdate` waits instead of reloading, and the feature calls
 * `flushDeferredUpdate` at its next safe point (a track boundary, a pause). Returns the remover.
 */
export function setUpdateGuard(guard: (() => boolean) | null): () => void {
  updateGuard = guard;
  return () => {
    if (updateGuard === guard) updateGuard = null;
  };
}

export function applyPendingUpdate() {
  if (updateGuard?.()) {
    set({ updateDeferred: true });
    return;
  }
  set({ updateDeferred: false });
  applyUpdate?.();
}

/** The guard's feature reached a safe point: run the update the member already asked for. */
export function flushDeferredUpdate(): boolean {
  if (!state.updateDeferred || updateGuard?.()) return false;
  applyPendingUpdate();
  return true;
}

/** Show the browser install dialog. Resolves to whether the user accepted. */
export async function promptInstall(): Promise<boolean> {
  if (!deferred) return false;
  const ev = deferred;
  deferred = null;
  set({ canInstall: false });
  await ev.prompt();
  return (await ev.userChoice).outcome === 'accepted';
}

export function usePwaState(): PwaState {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => state,
    () => state,
  );
}

/** Test hook: reset module state. */
export function __resetPwaState() {
  state = {
    updateReady: false,
    canInstall: false,
    installed: false,
    noticesBar: false,
    updateDeferred: false,
  };
  updateGuard = null;
  deferred = null;
  applyUpdate = null;
  for (const l of listeners) l();
}

/** True while an update the member asked for waits for a safe point (the player's track boundary). */
export const isUpdateDeferred = (): boolean => state.updateDeferred;

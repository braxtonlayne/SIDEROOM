/**
 * UTL-SHL-001: the drawer's open flag, in a module of its own so the gate can read and set it without loading the
 * drawer (the same split as `player/presence.ts`). The host chunk reads `drawerRequest` when it mounts.
 */
import { useSyncExternalStore } from 'react';

export type DrawerTab = 'goto' | 'tools';
export interface DrawerRequest {
  open: boolean;
  tab: DrawerTab;
  /** Changes on every open call, so an already open drawer is re-focused and re-targeted. */
  nonce: number;
}

let state: DrawerRequest = { open: false, tab: 'goto', nonce: 0 };
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((l) => l());

export function openDrawer(tab: DrawerTab = 'goto'): void {
  state = { open: true, tab, nonce: state.nonce + 1 };
  notify();
}
export function closeDrawer(): void {
  if (!state.open) return;
  state = { ...state, open: false };
  notify();
}
export const drawerRequest = (): DrawerRequest => state;
export function useDrawerRequest(): DrawerRequest {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    () => state,
    () => state,
  );
}

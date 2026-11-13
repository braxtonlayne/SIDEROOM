/**
 * SEC-AUD-168: the bug report dialog's open request, in a module of its own so the gate and the menu entries can read
 * and set it without loading the dialog (the same split as `tools/drawer/openState.ts`).
 */
import { useSyncExternalStore } from 'react';
import type { Rect, Size } from './covers';

/** One frame of the tab, as the grabber returns it. */
export interface CapturedFrame {
  source: CanvasImageSource;
  w: number;
  h: number;
}

/** What the dialog starts with. No `frame` means the dialog opens without a picture. */
export interface BugReportStart {
  frame?: CapturedFrame;
  /** Rectangles of `[data-sr-private]` elements, in viewport CSS pixels, read before the capture. */
  rects: Rect[];
  /** The viewport those rectangles are in. */
  viewport: Size;
  /** Why there is no frame, when a capture was tried. */
  note?: 'unsupported' | 'declined' | 'failed';
}

export interface BugReportRequest {
  open: boolean;
  /** Changes on every open call, so a second request starts a fresh dialog. */
  nonce: number;
  start: BugReportStart | null;
}

let state: BugReportRequest = { open: false, nonce: 0, start: null };
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((l) => l());

export function openBugReport(start: BugReportStart | null = null): void {
  state = { open: true, nonce: state.nonce + 1, start };
  notify();
}
export function closeBugReport(): void {
  if (!state.open) return;
  state = { ...state, open: false, start: null };
  notify();
}
export const bugReportRequest = (): BugReportRequest => state;
export function useBugReportRequest(): BugReportRequest {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    () => state,
    () => state,
  );
}

/** The open reports the Admin badge shows (set by the Admin app when it knows the number). */
let openCount: number | null = null;
const countListeners = new Set<() => void>();
export function setBugOpenCount(n: number | null): void {
  openCount = n;
  countListeners.forEach((l) => l());
}
export function useBugOpenCount(): number | null {
  return useSyncExternalStore(
    (cb) => {
      countListeners.add(cb);
      return () => countListeners.delete(cb);
    },
    () => openCount,
    () => openCount,
  );
}

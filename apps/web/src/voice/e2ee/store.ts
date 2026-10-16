/**
 * VOX-E2E-007: what the call UI shows about end-to-end encryption. The controller is the only
 * writer (from the call's MLS session); the shield popover reads it.
 */
import { create } from 'zustand';
import type { E2eeCallState } from './call';

export interface E2eeView {
  /** This call is end-to-end encrypted (the feature is on and the session started). */
  active: boolean;
  call: E2eeCallState | null;
  /** The viewer's own identity public key, hex (for the pairwise codes). */
  selfKey: string | null;
  /** D-250 pairwise code per participant, by user id (computed by the controller from WASM). */
  pairwise: Record<string, string>;
}

const EMPTY: E2eeView = { active: false, call: null, selfKey: null, pairwise: {} };

export const useE2ee = create<E2eeView>(() => EMPTY);

export function setE2ee(p: Partial<E2eeView>): void {
  useE2ee.setState(p);
}

export function resetE2ee(): void {
  useE2ee.setState(EMPTY, true);
}

// UX-SHOW-001: the Showroom's own state. It is a SEPARATE store: the real stores (session, lounge, query cache) are
// never written while the preview is on, so switching it off restores exactly what was there.
import { create } from 'zustand';
import { useClientFlag } from '../moderation/reports';
import { SHOWROOM_FLAG, SHOWROOM_STORAGE_KEY } from './flag';

function readChoice(): boolean {
  try {
    return localStorage.getItem(SHOWROOM_STORAGE_KEY) === '1';
  } catch {
    return false;
  }
}

function writeChoice(on: boolean): void {
  try {
    if (on) localStorage.setItem(SHOWROOM_STORAGE_KEY, '1');
    else localStorage.removeItem(SHOWROOM_STORAGE_KEY);
  } catch {
    // Private mode or blocked storage: the choice lasts for this page only.
  }
}

interface ShowroomState {
  /** The tester's choice on this device. */
  on: boolean;
  /** The last "this is a preview" message, shown in the banner for a few seconds. */
  hint: string | null;
}

export const useShowroom = create<ShowroomState>(() => ({ on: readChoice(), hint: null }));

export function setShowroom(on: boolean): void {
  writeChoice(on);
  useShowroom.setState({ on, hint: null });
}

let hintTimer: ReturnType<typeof setTimeout> | undefined;
const HINT_MS = 4000;

/**
 * Every attempt to change a fixture object ends here: nothing is sent, nothing is stored, and the banner says so.
 * `message` is already translated by the caller.
 */
export function previewHint(message: string): void {
  useShowroom.setState({ hint: message });
  if (hintTimer) clearTimeout(hintTimer);
  hintTimer = setTimeout(() => useShowroom.setState({ hint: null }), HINT_MS);
}

/** Whether this deployment offers the preview (the switch is on). */
export function useShowroomAvailable(): boolean {
  return useClientFlag(SHOWROOM_FLAG);
}

/** Whether the preview is showing: offered by the deployment AND chosen on this device. */
export function useShowroomActive(): boolean {
  const available = useShowroomAvailable();
  const on = useShowroom((s) => s.on);
  return available && on;
}

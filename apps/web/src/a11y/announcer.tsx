import { useSyncExternalStore } from 'react';

/**
 * App-wide ARIA live regions (XC-A11Y-002). Any code can call `announce('3 new messages')`; screen readers
 * hear it without moving focus. Two regions exist for the whole app, mounted once by `<LiveRegions />`:
 * polite (new messages, saved, uploaded) and assertive (call dropped, connection lost).
 */
type Politeness = 'polite' | 'assertive';

interface State {
  polite: string;
  assertive: string;
}

let state: State = { polite: '', assertive: '' };
const listeners = new Set<() => void>();
const timers: Partial<Record<Politeness, ReturnType<typeof setTimeout>>> = {};

function set(patch: Partial<State>) {
  state = { ...state, ...patch };
  for (const l of listeners) l();
}

/**
 * Announce a message. Repeating the same text still re-announces: the region is cleared first, then filled
 * on the next tick, which is what makes assistive tech treat it as a new change.
 */
export function announce(message: string, politeness: Politeness = 'polite'): void {
  clearTimeout(timers[politeness]);
  set({ [politeness]: '' });
  timers[politeness] = setTimeout(() => {
    set({ [politeness]: message });
    // Empty again after a while so the text is not found by virtual-cursor browsing.
    timers[politeness] = setTimeout(() => set({ [politeness]: '' }), 8000);
  }, 50);
}

const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

export function LiveRegions() {
  const s = useSyncExternalStore(
    subscribe,
    () => state,
    () => state,
  );
  return (
    <div className="sr-visually-hidden" data-testid="live-regions">
      <div role="status" aria-live="polite" aria-atomic="true">
        {s.polite}
      </div>
      <div role="alert" aria-live="assertive" aria-atomic="true">
        {s.assertive}
      </div>
    </div>
  );
}

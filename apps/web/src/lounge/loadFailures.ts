/**
 * SEC-AUD-046 (D-577): a server's channel list or member list that failed to load is shown where the list is
 * ("could not be loaded, Try again"), never as a toast: a toast is for an action the member just took. A failed
 * load is not retried in a loop; it is retried when the member presses Try again, and for every failed list when
 * the browser comes back online.
 */
import { create } from 'zustand';

export type LoadKind = 'server' | 'members';

interface LoadFailures {
  /** `<kind>:<guild id>` of each list whose last load failed. */
  failed: Record<string, true>;
}

export const useLoadFailures = create<LoadFailures>()(() => ({ failed: {} }));

const slot = (kind: LoadKind, guildId: string) => `${kind}:${guildId}`;

let listening = false;
function retryAllWhenOnline(): void {
  if (listening || typeof window === 'undefined') return;
  listening = true;
  window.addEventListener('online', () => {
    if (Object.keys(useLoadFailures.getState().failed).length)
      useLoadFailures.setState({ failed: {} });
  });
}

export function markLoadFailed(kind: LoadKind, guildId: string): void {
  retryAllWhenOnline();
  useLoadFailures.setState((s) => ({ failed: { ...s.failed, [slot(kind, guildId)]: true } }));
}

/** Forget the failure, so the list's loader runs again (the Try again button). */
export function retryLoad(kind: LoadKind, guildId: string): void {
  const key = slot(kind, guildId);
  if (!useLoadFailures.getState().failed[key]) return;
  useLoadFailures.setState((s) => {
    const failed = { ...s.failed };
    delete failed[key];
    return { failed };
  });
}

export function useLoadFailed(kind: LoadKind, guildId: string | undefined): boolean {
  return useLoadFailures((s) => (guildId ? s.failed[slot(kind, guildId)] === true : false));
}

/** Tests: forget every failure. */
export function resetLoadFailures(): void {
  useLoadFailures.setState({ failed: {} });
}

import { useSyncExternalStore } from 'react';
import { coopSession, type CoopSnapshot } from './session';

/** The co-op state of this window (hosting, looking, seated). */
export function useCoop(): CoopSnapshot {
  return useSyncExternalStore(coopSession.subscribe, coopSession.getSnapshot);
}

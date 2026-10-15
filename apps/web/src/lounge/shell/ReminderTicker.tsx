import { useContext, useEffect } from 'react';
import { LoungeDataContext } from '../data';
import { useSaved } from './savedMessages';

/** How often the lists are refreshed while the Lounge is open (reminders delivered and messages sent drop off). */
export const SAVED_REFRESH_MS = 60_000;

/**
 * Keeps the bookmarks, reminders and scheduled messages loaded (FTR-BKM-001, FTR-REM-001, FTR-SCH-001). Mounted
 * once in the shell. Reminders are delivered by the server as a direct message, so nothing fires here any more.
 */
export function ReminderTicker() {
  const mock = useContext(LoungeDataContext);
  useEffect(() => {
    if (mock) return;
    const load = () => void useSaved.getState().load();
    load();
    const timer = setInterval(load, SAVED_REFRESH_MS);
    return () => clearInterval(timer);
  }, [mock]);
  return null;
}

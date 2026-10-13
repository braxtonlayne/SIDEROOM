import { useContext, useEffect } from 'react';
import { LoungeDataContext } from '../data';
import { useNotifSettings } from './notifSettings';
import { useSocial } from './social';

/**
 * Loads what the Home screen and sidebars need beyond the gateway snapshot: friends and pending requests,
 * message requests, and DM notification settings. Skipped when a mock data provider is in charge.
 */
export function useShellBoot() {
  const mock = useContext(LoungeDataContext);
  const loadRelationships = useSocial((s) => s.loadRelationships);
  const loadRequests = useSocial((s) => s.loadMessageRequests);
  const loadNotif = useNotifSettings((s) => s.load);
  useEffect(() => {
    if (mock) return;
    void loadRelationships();
    void loadRequests();
    void loadNotif('@me');
  }, [mock, loadRelationships, loadRequests, loadNotif]);
}

import { useContext, useEffect } from 'react';
import { LoungeDataContext } from '../data';
import { useNotifSettings } from './notifSettings';
import { startSocialLive, useSocial } from './social';
import { useUserSettingsSync } from './userSettingsSync';

/**
 * Loads what the Home screen and sidebars need beyond the gateway snapshot: friends and pending requests,
 * message requests, and DM notification settings. Skipped when a mock data provider is in charge.
 */
export function useShellBoot() {
  const mock = useContext(LoungeDataContext);
  const loadRelationships = useSocial((s) => s.loadRelationships);
  const loadRequests = useSocial((s) => s.loadDms);
  const loadNotif = useNotifSettings((s) => s.load);
  useUserSettingsSync(!mock);
  useEffect(() => (mock ? undefined : startSocialLive()), [mock]);
  useEffect(() => {
    if (mock) return;
    void loadRelationships();
    void loadRequests();
    void loadNotif('@me');
  }, [mock, loadRelationships, loadRequests, loadNotif]);
}

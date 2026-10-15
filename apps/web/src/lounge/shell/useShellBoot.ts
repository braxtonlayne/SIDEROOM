import { useContext, useEffect } from 'react';
import { LoungeDataContext } from '../data';
import { useIgnores } from './ignores';
import { startIdleReporter } from './idleReporter';
import { useNotifSettings } from './notifSettings';
import { startSocialLive, useSocial } from './social';
import { useUserSettingsSync } from './userSettingsSync';
import { startThreadsLive } from '../threads/threadStore';
import { getMe } from '../../settings/user/api';
import { adoptAccountLocale } from '../../i18n/accountLocale';

/**
 * Loads what the Home screen and sidebars need beyond the gateway snapshot: friends and pending requests,
 * and DM notification settings. Skipped when a mock data provider is in charge.
 */
export function useShellBoot() {
  const mock = useContext(LoungeDataContext);
  const loadRelationships = useSocial((s) => s.loadRelationships);
  const loadNotif = useNotifSettings((s) => s.load);
  const loadIgnores = useIgnores((s) => s.load);
  useUserSettingsSync(!mock);
  // One locale (G-17): a device with no saved language takes the account's.
  useEffect(() => {
    if (mock) return;
    void getMe()
      .then((me) => adoptAccountLocale(me.locale))
      .catch(() => undefined);
  }, [mock]);
  useEffect(() => (mock ? undefined : startSocialLive()), [mock]);
  useEffect(() => (mock ? undefined : startThreadsLive()), [mock]);
  useEffect(() => (mock ? undefined : startIdleReporter()), [mock]);
  useEffect(() => {
    if (mock) return;
    void loadRelationships();
    void loadNotif('@me');
    void loadIgnores();
  }, [mock, loadRelationships, loadNotif, loadIgnores]);
}

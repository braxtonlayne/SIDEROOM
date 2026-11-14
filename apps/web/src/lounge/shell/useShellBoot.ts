import { useContext, useEffect } from 'react';
import { LoungeDataContext } from '../data';
import { useIgnores } from './ignores';
import { startIdleReporter } from './idleReporter';
import { useNotifSettings } from './notifSettings';
import { startSocialLive, useSocial } from './social';
import { useUserSettingsSync } from './userSettingsSync';
import { startThreadsLive } from '../threads/threadStore';
import { startPreviewsLive } from '../previewLive';
import { getMe } from '../../settings/user/api';
import { adoptAccountLocale } from '../../i18n/accountLocale';

export const BOOT_RETRY_MS = 30_000;

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
  useEffect(() => (mock ? undefined : startPreviewsLive()), [mock]);
  useEffect(() => (mock ? undefined : startIdleReporter()), [mock]);
  useEffect(() => {
    if (mock) return;
    void loadRelationships();
    void loadNotif('@me');
    void loadIgnores();
  }, [mock, loadRelationships, loadNotif, loadIgnores]);
  // The friend, block and ignore lists decide whose messages collapse: a failed boot read is retried
  // (every 30 s and when the network returns) instead of showing ignored people all session (SEC-AUD-036).
  const socialFailed = useSocial((s) => s.status === 'error');
  const ignoresFailed = useIgnores((s) => s.status === 'error');
  useEffect(() => {
    if (mock || (!socialFailed && !ignoresFailed)) return;
    const retry = () => {
      if (useSocial.getState().status === 'error') void loadRelationships();
      if (useIgnores.getState().status === 'error') void loadIgnores();
    };
    const timer = setInterval(retry, BOOT_RETRY_MS);
    window.addEventListener('online', retry);
    return () => {
      clearInterval(timer);
      window.removeEventListener('online', retry);
    };
  }, [mock, socialFailed, ignoresFailed, loadRelationships, loadIgnores]);
}

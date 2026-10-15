import { Button } from '@sideroom/ui';
import type { ReactNode } from 'react';
import { useEffect, useState } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { resendVerification } from '../../api/entrance';
import { t, useT } from '../../i18n';
import { applyPendingUpdate, usePwaState } from '../../pwa/state';
import { useConnectionStatus } from '../../store';
import { canDismiss, useActiveAnnouncements } from '../announcements';

/** True while the browser reports no network. */
export function useOnline(): boolean {
  const [online, setOnline] = useState(() =>
    typeof navigator === 'undefined' ? true : navigator.onLine,
  );
  useEffect(() => {
    const on = () => setOnline(true);
    const off = () => setOnline(false);
    window.addEventListener('online', on);
    window.addEventListener('offline', off);
    return () => {
      window.removeEventListener('online', on);
      window.removeEventListener('offline', off);
    };
  }, []);
  return online;
}

/** What the connection banner says for a gateway status and network state; null = nothing to show. */
export function connectionMessage(
  status: ReturnType<typeof useConnectionStatus>,
  online: boolean,
): { text: string; tone: 'info' | 'warn' } | null {
  if (!online)
    return {
      text: t('lounge.banners.offline'),
      tone: 'warn',
    };
  switch (status) {
    case 'connecting':
      return { text: t('lounge.banners.connecting'), tone: 'info' };
    case 'reconnecting':
      return { text: t('lounge.banners.reconnecting'), tone: 'warn' };
    case 'closed':
      return { text: t('lounge.banners.disconnected'), tone: 'warn' };
    default:
      return null;
  }
}

/** Connection state banner (LNG-SHL-008): connecting, reconnecting, offline. */
export function ConnectionBanner() {
  useT(); // re-render on locale change; the message text comes from the module-level t
  const status = useConnectionStatus();
  const online = useOnline();
  const m = connectionMessage(status, online);
  if (!m) return null;
  return (
    <div role="status" className="lng-banner" data-tone={m.tone}>
      {m.text}
    </div>
  );
}

type Sent = 'idle' | 'sending' | 'sent' | 'wait' | 'error';

interface Notice {
  key: string;
  tone: 'info' | 'warn' | 'danger';
  body: ReactNode;
  action?: ReactNode;
  dismissLabel: string;
  /** Absent for notices that cannot be dismissed (critical announcements). */
  onDismiss?: () => void;
}

/**
 * App-level notices bar (LNG-SHL-009): "verify your email" for accounts whose address is not confirmed, "a new
 * version is ready" when the service worker has an update waiting, and the platform announcements staff
 * published. Each notice can be dismissed; the region disappears when none is left.
 */
export function NoticesBar() {
  const t = useT();
  const [verified, setVerified] = useState(true);
  const [state, setState] = useState<Sent>('idle');
  const [verifyHidden, setVerifyHidden] = useState(false);
  const [updateHidden, setUpdateHidden] = useState(false);
  const pwa = usePwaState();
  // One source of truth for announcements (ADM-PLT-009): polled, critical ones cannot be dismissed.
  const announcements = useActiveAnnouncements();

  useEffect(() => {
    let live = true;
    apiFetch<{ verified?: boolean }>('/users/@me', { retries: 0 })
      .then((me) => live && setVerified(me.verified !== false))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, []);

  const resend = async () => {
    setState('sending');
    try {
      await resendVerification();
      setState('sent');
    } catch (e) {
      setState(e instanceof ApiError && e.status === 429 ? 'wait' : 'error');
    }
  };

  const notices: Notice[] = [];
  for (const a of announcements.announcements) {
    notices.push({
      key: `announcement-${a.id}`,
      tone: a.severity === 'critical' ? 'danger' : a.severity === 'warning' ? 'warn' : 'info',
      body: (
        <span>
          <strong>{a.title}</strong> {a.body}
        </span>
      ),
      dismissLabel: t('lounge.banners.dismissAnnouncement', { title: a.title }),
      ...(canDismiss(a) ? { onDismiss: () => announcements.dismiss(a.id) } : {}),
    });
  }
  if (pwa.updateReady && !updateHidden)
    notices.push({
      key: 'update',
      tone: 'info',
      body: <span>{t('lounge.banners.updateReady')}</span>,
      action: (
        <Button size="sm" variant="secondary" onClick={applyPendingUpdate}>
          {t('lounge.banners.updateReload')}
        </Button>
      ),
      dismissLabel: t('lounge.banners.dismissUpdate'),
      onDismiss: () => setUpdateHidden(true),
    });
  if (!verified && !verifyHidden)
    notices.push({
      key: 'verify',
      tone: 'info',
      body: (
        <span>
          {
            {
              idle: t('lounge.banners.verifyIdle'),
              sending: t('lounge.banners.verifySending'),
              sent: t('lounge.banners.verifySent'),
              wait: t('lounge.banners.verifyWait'),
              error: t('lounge.banners.verifyError'),
            }[state]
          }
        </span>
      ),
      action:
        state !== 'sent' && state !== 'sending' ? (
          <Button size="sm" variant="secondary" onClick={() => void resend()}>
            {t('lounge.banners.resendLink')}
          </Button>
        ) : undefined,
      dismissLabel: t('lounge.banners.dismissNotice'),
      onDismiss: () => setVerifyHidden(true),
    });

  if (notices.length === 0) return null;
  return (
    <div role="region" aria-label={t('lounge.banners.notices')}>
      {notices.map((n) => (
        <div key={n.key} className="lng-banner" data-tone={n.tone}>
          {n.body}
          {n.action}
          {n.onDismiss && (
            <button
              type="button"
              className="lng-icon-btn"
              aria-label={n.dismissLabel}
              onClick={n.onDismiss}
            >
              {'×'}
            </button>
          )}
        </div>
      ))}
    </div>
  );
}

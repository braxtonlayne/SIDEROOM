import { Button } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { resendVerification } from '../../api/entrance';
import { t, useT } from '../../i18n';
import { useConnectionStatus } from '../../store';

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

/**
 * App-level notices bar (LNG-SHL-009). Today it carries the "verify your email" notice for accounts whose
 * address is not confirmed; platform announcements and update prompts join it when their sources exist.
 */
export function NoticesBar() {
  const t = useT();
  const [verified, setVerified] = useState(true);
  const [state, setState] = useState<Sent>('idle');
  const [hidden, setHidden] = useState(false);

  useEffect(() => {
    let live = true;
    apiFetch<{ verified?: boolean }>('/users/@me', { retries: 0 })
      .then((me) => live && setVerified(me.verified !== false))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, []);

  if (verified || hidden) return null;

  const resend = async () => {
    setState('sending');
    try {
      await resendVerification();
      setState('sent');
    } catch (e) {
      setState(e instanceof ApiError && e.status === 429 ? 'wait' : 'error');
    }
  };
  const text = {
    idle: t('lounge.banners.verifyIdle'),
    sending: t('lounge.banners.verifySending'),
    sent: t('lounge.banners.verifySent'),
    wait: t('lounge.banners.verifyWait'),
    error: t('lounge.banners.verifyError'),
  }[state];

  return (
    <div
      role="region"
      aria-label={t('lounge.banners.notices')}
      className="lng-banner"
      data-tone="info"
    >
      <span>{text}</span>
      {state !== 'sent' && state !== 'sending' && (
        <Button size="sm" variant="secondary" onClick={() => void resend()}>
          {t('lounge.banners.resendLink')}
        </Button>
      )}
      <button
        type="button"
        className="lng-icon-btn"
        aria-label={t('lounge.banners.dismissNotice')}
        onClick={() => setHidden(true)}
      >
        {'×'}
      </button>
    </div>
  );
}

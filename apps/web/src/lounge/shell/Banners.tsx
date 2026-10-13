import { Button } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { resendVerification } from '../../api/entrance';
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
      text: 'You are offline. Showing what was loaded; messages will not send.',
      tone: 'warn',
    };
  switch (status) {
    case 'connecting':
      return { text: 'Connecting…', tone: 'info' };
    case 'reconnecting':
      return { text: 'Reconnecting…', tone: 'warn' };
    case 'closed':
      return { text: 'Disconnected. Trying again shortly.', tone: 'warn' };
    default:
      return null;
  }
}

/** Connection state banner (LNG-SHL-008): connecting, reconnecting, offline. */
export function ConnectionBanner() {
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
    idle: 'Verify your email to keep your account safe.',
    sending: 'Sending a fresh link…',
    sent: 'Check your inbox for the link.',
    wait: 'A link was sent a moment ago. Try again in a minute.',
    error: 'We could not send the link. Try again in a moment.',
  }[state];

  return (
    <div role="region" aria-label="Notices" className="lng-banner" data-tone="info">
      <span>{text}</span>
      {state !== 'sent' && state !== 'sending' && (
        <Button size="sm" variant="secondary" onClick={() => void resend()}>
          Resend link
        </Button>
      )}
      <button
        type="button"
        className="lng-icon-btn"
        aria-label="Dismiss notice"
        onClick={() => setHidden(true)}
      >
        {'×'}
      </button>
    </div>
  );
}

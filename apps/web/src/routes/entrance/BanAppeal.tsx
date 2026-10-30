// The ban screen's appeal form (MOD-APL-004; D-215). A banned account has no session, so the credentials typed
// into the sign-in form prove it. One appeal per decision; there is no second appeal.
import { GoldButton, TextArea } from '@sideroom/ui';
import { useState } from 'react';
import { ApiError } from '../../api/client';
import { useT } from '../../i18n/react';
import { fileAppeal } from '../../api/entrance';
import { minutesFromSeconds } from '../../lib/validation';

/** The sign-in answer for a banned account (`SrPlatformBanned`, 900008). */
export function isBannedError(e: unknown): boolean {
  return e instanceof ApiError && e.status === 403 && e.body?.code === 900008;
}

export function BanAppeal({ login, password }: { login: string; password: string }) {
  const t = useT();
  const [message, setMessage] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [sent, setSent] = useState(false);

  const send = async () => {
    if (!message.trim() || busy) return;
    setBusy(true);
    setError(null);
    try {
      await fileAppeal({ login, password, message: message.trim() });
      setSent(true);
    } catch (e) {
      if (e instanceof ApiError && e.status === 409) setError(t('entrance.appeal.already'));
      else if (e instanceof ApiError && e.status === 429)
        setError(t('entrance.login.tooMany', { minutes: minutesFromSeconds(e.retryAfter) }));
      else if (e instanceof ApiError && e.status === 401) setError(t('entrance.login.mismatch'));
      else setError(t('entrance.appeal.failed'));
    } finally {
      setBusy(false);
    }
  };

  if (sent)
    return (
      <p role="status" className="ent-appeal">
        {t('entrance.appeal.sent')}
      </p>
    );
  return (
    <div className="ent-appeal">
      <p>{t('entrance.appeal.intro')}</p>
      <TextArea
        label={t('entrance.appeal.label')}
        value={message}
        rows={4}
        maxLength={2000}
        onChange={(e) => setMessage(e.target.value)}
      />
      {error && <p role="alert">{error}</p>}
      <GoldButton type="button" disabled={busy || !message.trim()} onClick={() => void send()}>
        {t('entrance.appeal.send')}
      </GoldButton>
    </div>
  );
}

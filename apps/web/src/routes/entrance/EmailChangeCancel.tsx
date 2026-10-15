import { Link, useSearch } from '@tanstack/react-router';
import { useEffect, useRef, useState } from 'react';
import { ApiError } from '../../api/client';
import { cancelEmailChange } from '../../api/entrance';
import { AuthSplit, FormHead } from '../../components/AuthSplit';
import { useT } from '../../i18n/react';
import { minutesFromSeconds } from '../../lib/validation';

type State = 'working' | 'missing' | 'done' | 'invalid' | 'throttled' | 'error';

/**
 * The "This wasn't me" link from the email-change notice (D-167): `/email-change/cancel?token=…`. The token
 * is single use, so it is posted once. While the change is still pending it is cancelled; once it has
 * completed, every session and trusted device is signed out instead (the server decides which).
 */
export function EmailChangeCancel() {
  const t = useT();
  const { token } = useSearch({ strict: false }) as { token?: string };
  const [state, setState] = useState<State>(token ? 'working' : 'missing');
  const [retryAfter, setRetryAfter] = useState<number | null>(null);
  const sent = useRef(false);

  useEffect(() => {
    if (!token || sent.current) return;
    sent.current = true;
    cancelEmailChange(token)
      .then(() => setState('done'))
      .catch((e: unknown) => {
        if (e instanceof ApiError && e.status === 429) {
          setRetryAfter(e.retryAfter);
          setState('throttled');
        } else setState(e instanceof ApiError && e.status < 500 ? 'invalid' : 'error');
      });
  }, [token]);

  return (
    <AuthSplit
      eyebrow={t('entrance.emailCancel.eyebrow')}
      heading={t('entrance.emailCancel.heading')}
      accent={t('entrance.emailCancel.headingAccent')}
      copy={t('entrance.emailCancel.copy')}
      footLabel={t('entrance.emailCancel.footLabel')}
    >
      <FormHead
        eyebrow={t('entrance.emailCancel.formEyebrow')}
        title={t('entrance.emailCancel.title')}
      />
      {state === 'working' && (
        <p role="status" className="ent-sub">
          {t('entrance.common.oneMoment')}
        </p>
      )}
      {state === 'done' && (
        <>
          <p role="status" className="ent-sub">
            {t('entrance.emailCancel.done')}
          </p>
          <p className="ent-sub">
            {t('entrance.emailCancel.adviceBefore')}{' '}
            <Link to="/forgot">{t('entrance.emailCancel.adviceLink')}</Link>{' '}
            {t('entrance.emailCancel.adviceAfter')}
          </p>
        </>
      )}
      {state === 'missing' && (
        <p role="alert" className="ent-sub">
          {t('entrance.emailCancel.missing')}
        </p>
      )}
      {state === 'invalid' && (
        <p role="alert" className="ent-sub">
          {t('entrance.emailCancel.invalid')}
        </p>
      )}
      {state === 'throttled' && (
        <p role="alert" className="ent-sub">
          {t('entrance.common.tooManyTries', { minutes: minutesFromSeconds(retryAfter) })}
        </p>
      )}
      {state === 'error' && (
        <p role="alert" className="ent-sub">
          {t('entrance.common.unreachable')}
        </p>
      )}
      <p className="ent-footnote">
        <Link to="/login">{t('entrance.common.backToSignIn')}</Link>
      </p>
    </AuthSplit>
  );
}

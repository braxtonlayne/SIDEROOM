import { Notice } from '@sideroom/ui';
import { useSearch } from '@tanstack/react-router';
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
          <Notice variant="success" className="ent-notice">
            {t('entrance.emailCancel.done')}
          </Notice>
          <p className="ent-sub">
            {t('entrance.emailCancel.adviceBefore')}{' '}
            <a href="/forgot">{t('entrance.emailCancel.adviceLink')}</a>{' '}
            {t('entrance.emailCancel.adviceAfter')}
          </p>
        </>
      )}
      {state === 'missing' && (
        <Notice variant="warning" className="ent-notice">
          {t('entrance.emailCancel.missing')}
        </Notice>
      )}
      {state === 'invalid' && (
        <Notice variant="warning" className="ent-notice">
          {t('entrance.emailCancel.invalid')}
        </Notice>
      )}
      {state === 'throttled' && (
        <Notice variant="warning" className="ent-notice">
          {t('entrance.common.tooManyTries', { minutes: minutesFromSeconds(retryAfter) })}
        </Notice>
      )}
      {state === 'error' && (
        <Notice variant="warning" className="ent-notice">
          {t('entrance.common.unreachable')}
        </Notice>
      )}
      <p className="ent-footnote">
        <a href="/login">{t('entrance.common.backToSignIn')}</a>
      </p>
    </AuthSplit>
  );
}

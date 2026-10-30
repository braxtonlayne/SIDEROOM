import { useSearch } from '@tanstack/react-router';
import { useEffect, useRef, useState } from 'react';
import { ApiError } from '../../api/client';
import { loginWithLink } from '../../api/entrance';
import { AuthSplit, FormHead } from '../../components/AuthSplit';
import { useT } from '../../i18n/react';
import { enterApp } from '../../lib/enterApp';
import { minutesFromSeconds } from '../../lib/validation';

type State =
  | { kind: 'signing-in' }
  | { kind: 'missing' }
  | { kind: 'expired' }
  | { kind: 'throttled'; retryAfter: number | null }
  | { kind: 'error' };

/**
 * Owner one-time login link: `/login/link?token=…`. Posts the token once, shows progress, and on success
 * continues to the Lounge. A failed link explains how to get a new one; it never leaves a session behind.
 */
export function LoginLink() {
  const t = useT();
  const { token } = useSearch({ strict: false }) as { token?: string };
  const [state, setState] = useState<State>(() =>
    token ? { kind: 'signing-in' } : { kind: 'missing' },
  );
  const sent = useRef(false);

  useEffect(() => {
    if (!token || sent.current) return;
    sent.current = true;
    loginWithLink(token)
      .then(() => enterApp())
      .catch((e: unknown) => {
        if (e instanceof ApiError && e.status === 429)
          setState({ kind: 'throttled', retryAfter: e.retryAfter });
        else if (e instanceof ApiError && e.status < 500) setState({ kind: 'expired' });
        else setState({ kind: 'error' });
      });
  }, [token]);

  return (
    <AuthSplit
      eyebrow={t('entrance.loginLink.eyebrow')}
      heading={t('entrance.loginLink.heading')}
      accent={t('entrance.loginLink.headingAccent')}
      copy={t('entrance.loginLink.copy')}
      footLabel={t('entrance.loginLink.footLabel')}
    >
      <FormHead
        eyebrow={t('entrance.loginLink.formEyebrow')}
        title={t('entrance.loginLink.title')}
      />
      {state.kind === 'signing-in' && (
        <p role="status" className="ent-sub">
          {t('entrance.common.oneMoment')}
        </p>
      )}
      {state.kind === 'missing' && (
        <p role="alert" className="ent-sub">
          {t('entrance.loginLink.missing')}
        </p>
      )}
      {state.kind === 'expired' && (
        <p role="alert" className="ent-sub">
          {t('entrance.loginLink.expired')}
        </p>
      )}
      {state.kind === 'throttled' && (
        <p role="alert" className="ent-sub">
          {t('entrance.common.tooManyTries', { minutes: minutesFromSeconds(state.retryAfter) })}
        </p>
      )}
      {state.kind === 'error' && (
        <p role="alert" className="ent-sub">
          {t('entrance.common.unreachable')}
        </p>
      )}
      <p className="ent-footnote">
        <a href="/login">{t('entrance.loginLink.usePassword')}</a>
      </p>
    </AuthSplit>
  );
}

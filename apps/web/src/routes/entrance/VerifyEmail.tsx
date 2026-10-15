import { Link, useParams } from '@tanstack/react-router';
import { useEffect, useRef, useState } from 'react';
import { ApiError } from '../../api/client';
import { verifyEmail } from '../../api/entrance';
import { AuthSplit, FormHead } from '../../components/AuthSplit';
import { useT } from '../../i18n/react';

type State = 'verifying' | 'done' | 'expired' | 'error';

/** ENT-UI-020: `/verify-email/:token`. The token is single use, so the request is sent once. */
export function VerifyEmail() {
  const t = useT();
  const { token } = useParams({ strict: false }) as { token: string };
  const [state, setState] = useState<State>('verifying');
  const sent = useRef(false);

  useEffect(() => {
    if (sent.current) return;
    sent.current = true;
    verifyEmail(token)
      .then(() => setState('done'))
      .catch((e: unknown) => {
        setState(e instanceof ApiError && e.status === 400 ? 'expired' : 'error');
      });
  }, [token]);

  return (
    <AuthSplit
      eyebrow={t('entrance.verifyEmail.eyebrow')}
      heading={t('entrance.verifyEmail.heading')}
      accent={t('entrance.verifyEmail.headingAccent')}
      copy={t('entrance.verifyEmail.copy')}
      footLabel={t('entrance.verifyEmail.footLabel')}
    >
      <FormHead
        eyebrow={t('entrance.verifyEmail.formEyebrow')}
        title={t('entrance.verifyEmail.title')}
      />
      {state === 'verifying' && (
        <p role="status" className="ent-sub">
          {t('entrance.common.oneMoment')}
        </p>
      )}
      {state === 'done' && (
        <p role="status" className="ent-sub">
          {t('entrance.verifyEmail.done')}
        </p>
      )}
      {state === 'expired' && (
        <p role="alert" className="ent-sub">
          {t('entrance.verifyEmail.expired')}
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

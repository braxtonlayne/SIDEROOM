import { useT } from '../i18n/react';
import { useCompliancePublic } from '../lib/compliancePublic';

/**
 * The few self-service answers a visitor needs (sign-in trouble, a lost second factor, no invitation, the
 * house notes), folded into the sign-in page as a collapsible section. They used to be the public /help page;
 * the handbook now sits behind the login (D-385). Links to other pages are plain anchors: each page is its
 * own surface, loaded as a document.
 */
export function SignInHelp() {
  const t = useT();
  const { operator_contact: contact } = useCompliancePublic();
  return (
    <details className="ent-signin-help">
      <summary>{t('entrance.support.signinHelp.summary')}</summary>
      <section aria-labelledby="help-signin">
        <h2 id="help-signin">{t('entrance.support.signin.title')}</h2>
        <p>
          {t('entrance.support.signin.body')}{' '}
          <a href="/forgot">{t('entrance.support.signin.link')}</a>
        </p>
      </section>
      <section aria-labelledby="help-2fa">
        <h2 id="help-2fa">{t('entrance.support.twofa.title')}</h2>
        <p>{t('entrance.support.twofa.body')}</p>
      </section>
      <section aria-labelledby="help-invite">
        <h2 id="help-invite">{t('entrance.support.invite.title')}</h2>
        <p>
          {t('entrance.support.invite.body')}{' '}
          <a href="/no-invitation">{t('entrance.support.invite.link')}</a>
        </p>
      </section>
      <section aria-labelledby="help-rules">
        <h2 id="help-rules">{t('entrance.support.rules.title')}</h2>
        <p>
          {t('entrance.support.rules.body')}{' '}
          <a href="/house-notes">{t('entrance.support.rules.link')}</a>
        </p>
      </section>
      {contact && (
        <section aria-labelledby="help-copyright">
          <h2 id="help-copyright">{t('entrance.support.copyright.title')}</h2>
          <p>
            {t('entrance.support.copyright.body')} <a href={`mailto:${contact}`}>{contact}</a>
          </p>
        </section>
      )}
    </details>
  );
}

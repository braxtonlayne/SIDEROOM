import { Link } from '@tanstack/react-router';
import { useEffect } from 'react';
import { EntranceShell } from '../../components/EntranceShell';
import { useT } from '../../i18n/react';
import { useCompliancePublic } from '../../lib/compliancePublic';
import { EntranceMarkdown, useEntranceCopy } from './entranceCopy';

export function NoInvitation() {
  const t = useT();
  const custom = useEntranceCopy()['no-invitation'];
  return (
    <EntranceShell nav="member-access" footerRight="House notes">
      <main className="ent-prose">
        <h1>{t('entrance.static.noInvitation.title')}</h1>
        {custom ? (
          <EntranceMarkdown source={custom} />
        ) : (
          <p>{t('entrance.static.noInvitation.body')}</p>
        )}
      </main>
    </EntranceShell>
  );
}

/** Anchors (`#privacy`) arrive after a full page load, before React has rendered the target: scroll once it exists. */
function useScrollToHash() {
  useEffect(() => {
    const id = decodeURIComponent(window.location.hash.slice(1));
    if (id) document.getElementById(id)?.scrollIntoView?.({ block: 'start' });
  }, []);
}

/**
 * House notes (ENT-UI-010): rules, a plain-language privacy summary (docs/09), terms, image credits. The
 * anchors are linked from the Library footer. Operators will be able to replace the copy from Admin → Entrance
 * (ENT-UI-025); until then these are the defaults.
 */
export function HouseNotes() {
  const t = useT();
  const custom = useEntranceCopy()['house-notes'];
  useScrollToHash();
  return (
    <EntranceShell nav="member-access" footerRight="House notes">
      <main className="ent-prose ent-notes">
        <h1>{t('entrance.houseNotes.title')}</h1>
        {custom ? <EntranceMarkdown source={custom} /> : <DefaultHouseNotes />}
      </main>
    </EntranceShell>
  );
}

function DefaultHouseNotes() {
  const t = useT();
  const { operator_country: country, operator_contact: contact } = useCompliancePublic();
  return (
    <>
      <p>{t('entrance.houseNotes.intro')}</p>

      <section id="rules" aria-labelledby="notes-rules">
        <h2 id="notes-rules">{t('entrance.houseNotes.rules.title')}</h2>
        <ol>
          <li>{t('entrance.houseNotes.rules.1')}</li>
          <li>{t('entrance.houseNotes.rules.2')}</li>
          <li>{t('entrance.houseNotes.rules.3')}</li>
          <li>{t('entrance.houseNotes.rules.4')}</li>
          <li>{t('entrance.houseNotes.rules.5')}</li>
        </ol>
      </section>

      <section id="privacy" aria-labelledby="notes-privacy">
        <h2 id="notes-privacy">{t('entrance.houseNotes.privacy.title')}</h2>
        <p>{t('entrance.houseNotes.privacy.body1')}</p>
        <p>{t('entrance.houseNotes.privacy.body2')}</p>
        <p>{t('entrance.houseNotes.privacy.body3')}</p>
        {country && <p>{t('entrance.houseNotes.privacy.operatorCountry', { country })}</p>}
        {contact && (
          <p>
            {t('entrance.houseNotes.privacy.operatorContact')}{' '}
            <a href={`mailto:${contact}`}>{contact}</a>
          </p>
        )}
      </section>

      <section id="terms" aria-labelledby="notes-terms">
        <h2 id="notes-terms">{t('entrance.houseNotes.terms.title')}</h2>
        <p>{t('entrance.houseNotes.terms.body')}</p>
      </section>

      <section id="image-credits" aria-labelledby="notes-credits">
        <h2 id="notes-credits">{t('entrance.houseNotes.credits.title')}</h2>
        <p>{t('entrance.houseNotes.credits.door')}</p>
        <p>
          {t('entrance.houseNotes.credits.fonts')}{' '}
          <a href="/fonts/README.md">{t('entrance.houseNotes.credits.licences')}</a>
        </p>
      </section>
    </>
  );
}

/** The few self-service answers, then "ask whoever invited you"; the body of the /help page. */
export function HelpSections() {
  const t = useT();
  const { operator_contact: contact } = useCompliancePublic();
  return (
    <>
      <section aria-labelledby="help-signin">
        <h2 id="help-signin">{t('entrance.support.signin.title')}</h2>
        <p>
          {t('entrance.support.signin.body')}{' '}
          <Link to="/forgot">{t('entrance.support.signin.link')}</Link>
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
          <Link to="/no-invitation">{t('entrance.support.invite.link')}</Link>
        </p>
      </section>
      <section aria-labelledby="help-rules">
        <h2 id="help-rules">{t('entrance.support.rules.title')}</h2>
        <p>
          {t('entrance.support.rules.body')}{' '}
          <Link to="/house-notes">{t('entrance.support.rules.link')}</Link>
        </p>
      </section>
      {contact && (
        <section aria-labelledby="help-copyright">
          <h2 id="help-copyright">{t('entrance.support.copyright.title')}</h2>
          <p>
            {t('entrance.support.copyright.body')}{' '}
            <a href={`mailto:${contact}`}>{contact}</a>
          </p>
        </section>
      )}
    </>
  );
}

/** `/help` (D-270): the public help answers; `/support` is the donation page and only links here. */
export function Help() {
  const t = useT();
  return (
    <EntranceShell nav="member-access" footerRight="House notes">
      <main className="ent-prose ent-notes">
        <h1>{t('entrance.support.title')}</h1>
        <p>{t('entrance.support.intro')}</p>
        <HelpSections />
      </main>
    </EntranceShell>
  );
}

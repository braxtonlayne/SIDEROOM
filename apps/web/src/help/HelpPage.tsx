import { EntranceShell } from '../components/EntranceShell';
import { useT } from '../i18n';
import { HelpSections } from '../routes/entrance/Static';
import { Handbook } from './Handbook';

/**
 * `/help` (D-270, RDY-ONB-002): the member handbook, then the few self-service answers; `/support` is the
 * donation page and only links here. It lives outside `routes/entrance` because the handbook is long: it waits
 * for the full message catalog instead of riding on the first-paint fragments (see `i18n/corePaths.ts`).
 */
export function Help() {
  const t = useT();
  return (
    <EntranceShell nav="member-access" footerRight="House notes">
      <main className="ent-prose ent-notes">
        <h1>{t('entrance.support.title')}</h1>
        <Handbook />
        <p>{t('entrance.support.intro')}</p>
        <HelpSections />
        <section aria-labelledby="help-notices">
          <h2 id="help-notices">{t('entrance.support.notices.title')}</h2>
          <p>
            {t('entrance.support.notices.body')}{' '}
            <a href="/THIRD_PARTY_NOTICES.md" target="_blank" rel="noreferrer">
              {t('entrance.support.notices.link')}
            </a>
          </p>
        </section>
      </main>
    </EntranceShell>
  );
}

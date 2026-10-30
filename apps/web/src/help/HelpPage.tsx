import { EntranceShell } from '../components/EntranceShell';
import { useT } from '../i18n';
import { Handbook } from './Handbook';

/**
 * `/help` (D-270, RDY-ONB-002): the member handbook. Since D-385 it is an app page, served to a session only;
 * the sign-in answers moved to the sign-in page (`components/SignInHelp.tsx`). `/support` is the donation page
 * and only links here. It lives outside `routes/entrance` because the handbook is long: it waits
 * for the full message catalog instead of riding on the first-paint fragments (see `i18n/corePaths.ts`).
 */
export function Help() {
  const t = useT();
  return (
    <EntranceShell nav="none" footerRight="House notes">
      <main className="ent-prose ent-notes">
        <h1>{t('entrance.support.title')}</h1>
        <Handbook />
        <section aria-labelledby="help-notices">
          <h2 id="help-notices">{t('entrance.support.notices.title')}</h2>
          <p>
            {t('entrance.support.notices.body')}{' '}
            <a href="/app/THIRD_PARTY_NOTICES.md" target="_blank" rel="noreferrer">
              {t('entrance.support.notices.link')}
            </a>
          </p>
        </section>
      </main>
    </EntranceShell>
  );
}

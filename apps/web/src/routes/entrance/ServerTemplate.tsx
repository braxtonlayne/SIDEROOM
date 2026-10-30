import { ThemeProvider } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { getMe } from '../../api/entrance';
import { useT } from '../../i18n/react';
import { enterApp } from '../../lib/enterApp';
import { useEntranceThemes } from '../../lib/entranceTheme';
import '../../components/entrance.css';

/**
 * Public side of a server template (LNG-SSET-071), `/template/:code`. The preview needs a session, so a
 * visitor is sent to log in and comes back; a signed-in member is handed to the app, which shows the preview.
 */
export function ServerTemplate({ code }: { code: string }) {
  const t = useT();
  const themes = useEntranceThemes();
  const [state, setState] = useState<'loading' | 'login'>('loading');

  useEffect(() => {
    let live = true;
    getMe().then(
      () => {
        if (live) enterApp(`/app?goto=${encodeURIComponent(`/template/${code}`)}`);
      },
      () => live && setState('login'),
    );
    return () => {
      live = false;
    };
  }, [code]);

  return (
    <ThemeProvider
      section="entrance"
      platformDefault="sideroom"
      perSection={themes}
      className="ent-invite-page"
    >
      <main className="ent-invite-card" aria-busy={state === 'loading'}>
        {state === 'loading' ? (
          <p role="status">{t('entrance.template.loading')}</p>
        ) : (
          <>
            <h1>{t('entrance.template.loginTitle')}</h1>
            <a
              href={`/login?next=${encodeURIComponent(`/template/${code}`)}`}
              className="ent-invite-cta"
            >
              {t('entrance.template.login')}
            </a>
          </>
        )}
      </main>
    </ThemeProvider>
  );
}

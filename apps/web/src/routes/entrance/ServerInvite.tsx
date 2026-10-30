import { ThemeProvider } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { getMe } from '../../api/entrance';
import { useT } from '../../i18n/react';
import { enterApp } from '../../lib/enterApp';
import '../../components/entrance.css';

/**
 * Public side of a server invite (D-021, D-385). The preview needs a session, so a visitor without one sees
 * only the generic card with "Log in to accept" (no register button, no link to the landing page, nothing
 * about the server). A signed-in member is handed to the app, which shows the real card and the accept button.
 */
export function ServerInvite({ code }: { code: string }) {
  const t = useT();
  const [state, setState] = useState<'loading' | 'visitor'>('loading');

  useEffect(() => {
    let live = true;
    getMe().then(
      () => {
        if (live) enterApp(`/app?goto=${encodeURIComponent(`/invite/${code}`)}`);
      },
      () => live && setState('visitor'),
    );
    return () => {
      live = false;
    };
  }, [code]);

  return (
    <ThemeProvider section="entrance" platformDefault="sideroom" className="ent-invite-page">
      <main className="ent-invite-card" aria-busy={state === 'loading'}>
        {state === 'loading' ? (
          <p role="status">{t('entrance.serverInvite.loading')}</p>
        ) : (
          <>
            <p>{t('entrance.serverInvite.invited')}</p>
            <h1>{t('entrance.serverInvite.someServer')}</h1>
            <a
              href={`/login?next=${encodeURIComponent(`/invite/${code}`)}`}
              className="ent-invite-cta"
            >
              {t('entrance.serverInvite.login')}
            </a>
          </>
        )}
      </main>
    </ThemeProvider>
  );
}

import { EmptyState } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useT } from '../i18n';
import { LoungeThemeProvider } from '../settings/user/LoungeThemeProvider';
import './notFound.css';

/**
 * The app's "nothing at this address" page (XC-VIS-001). It used to be one unstyled line in the top corner with no
 * way back; it is now a themed page with a link to the Lounge. Its own chunk: the router only names it.
 */
export function NotFoundPage() {
  const t = useT();
  return (
    <LoungeThemeProvider className="app-notfound">
      <main className="app-notfound-main">
        <EmptyState
          title={
            <span role="heading" aria-level={1}>
              {t('app.notFound.title')}
            </span>
          }
          description={t('app.notFound.body')}
          action={
            <Link to="/channels" className="app-notfound-link">
              {t('app.notFound.back')}
            </Link>
          }
        />
      </main>
    </LoungeThemeProvider>
  );
}

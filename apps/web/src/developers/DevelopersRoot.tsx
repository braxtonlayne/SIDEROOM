import { ThemeProvider, ToastProvider } from '@sideroom/ui';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Link, Outlet } from '@tanstack/react-router';
import { useState } from 'react';
import { useT } from '../i18n';
import { useAppliedTheme } from '../settings/user/useAppliedTheme';
import './developers.css';

/** The section chrome: brand bar and section navigation. Mobile wraps the nav under the brand. */
function Shell() {
  const t = useT();
  return (
    <div className="dev-frame">
      <a className="dev-skip" href="#dev-main">
        {t('developers.shell.skipToContent')}
      </a>
      <header className="dev-bar">
        <span className="dev-brand">{t('developers.shell.brand')}</span>
        <span aria-hidden="true" className="dev-slash">
          /
        </span>
        <span className="dev-brand-sub">{t('developers.shell.brandSub')}</span>
        <nav aria-label={t('developers.shell.navLabel')} className="dev-nav">
          <Link to="/developers" activeOptions={{ exact: true }} className="dev-nav-link">
            {t('developers.shell.navOverview')}
          </Link>
          <Link to="/developers/applications" className="dev-nav-link">
            {t('developers.shell.navApplications')}
          </Link>
          <Link to="/developers/packages" className="dev-nav-link">
            {t('developers.shell.navPackages')}
          </Link>
          <Link to="/developers/docs" className="dev-nav-link">
            {t('developers.shell.navDocumentation')}
          </Link>
          <Link to="/channels" className="dev-nav-link dev-nav-exit">
            {t('developers.shell.backToLounge')}
          </Link>
        </nav>
      </header>
      <main id="dev-main" className="dev-main" tabIndex={-1}>
        <Outlet />
      </main>
    </div>
  );
}

/** Own query cache and theme, loaded lazily by routes.tsx (own bundle budget entry). */
export function DevelopersRoot() {
  const theme = useAppliedTheme();
  const [client] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            retry: import.meta.env.MODE === 'test' ? false : 1,
            refetchOnWindowFocus: false,
          },
        },
      }),
  );
  return (
    <QueryClientProvider client={client}>
      <ThemeProvider
        section="developer"
        perSection={theme.perSection}
        appearance={theme.appearance}
        style={theme.style}
        {...theme.data}
        className="dev-root"
      >
        <ToastProvider>
          <Shell />
        </ToastProvider>
      </ThemeProvider>
    </QueryClientProvider>
  );
}

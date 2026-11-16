import { ThemeProvider, ToastProvider } from '@sideroom/ui';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Link, Outlet, useRouterState } from '@tanstack/react-router';
import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n';
import { useAppliedTheme } from '../settings/user/useAppliedTheme';
import { useCurrentLinkInView } from '../study/useCurrentLinkInView';
import './developers.css';

/** The section chrome: brand bar and section navigation. Mobile wraps the nav under the brand. */
function Shell() {
  const t = useT();
  const navRef = useRef<HTMLElement>(null);
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  useCurrentLinkInView(navRef, "[aria-current='page']", pathname);
  // `data-overflow` names the edge(s) of the phone nav that hide links (the fade in developers.css, D-825).
  useEffect(() => {
    const nav = navRef.current;
    if (!nav) return;
    const sync = () => {
      const max = nav.scrollWidth - nav.clientWidth;
      const pos = Math.abs(nav.scrollLeft);
      if (max <= 1) nav.removeAttribute('data-overflow');
      else if (pos > 1 && pos < max - 1) nav.setAttribute('data-overflow', 'both');
      else if (pos > 1) nav.setAttribute('data-overflow', 'start');
      else nav.setAttribute('data-overflow', 'end');
    };
    sync();
    nav.addEventListener('scroll', sync, { passive: true });
    const ro = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(sync);
    ro?.observe(nav);
    return () => {
      nav.removeEventListener('scroll', sync);
      ro?.disconnect();
    };
  }, [pathname]);
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
        <nav ref={navRef} aria-label={t('developers.shell.navLabel')} className="dev-nav">
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

/**
 * Own query cache and theme, loaded lazily by routes.tsx (own bundle budget entry). The portal follows the
 * viewer's Lounge theme (light or dark) like the rest of the app area; a platform default set for the
 * `developer` section still wins.
 */
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
        userGlobal={theme.perSection.lounge ?? theme.userGlobal}
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

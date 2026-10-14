import { ThemeProvider, ToastProvider } from '@sideroom/ui';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Outlet, useRouter, useRouterState } from '@tanstack/react-router';
import { useState, type MouseEvent } from 'react';
import { AgeGateProvider } from '../lib/ageGate';
import { useAppliedTheme } from '../settings/user/useAppliedTheme';
import { CinemaShell, useRequireSession } from './Shell';
import './cinema.css';

/** The player renders full screen, outside the section chrome (owned by p6-cinema-player). */
const isPlayer = (path: string) =>
  path.startsWith('/cinema/watch/') || path.startsWith('/cinema/tv/watch/');

function Frame() {
  const player = useRouterState({ select: (s) => isPlayer(s.location.pathname) });
  const unauthorized = useRequireSession();
  if (unauthorized) return null;
  return player ? <Outlet /> : <CinemaShell />;
}

/** The section frame: own query cache, age gate, theme and chrome. Loaded lazily by routes.tsx. */
export function CinemaRoot() {
  const theme = useAppliedTheme();
  const router = useRouter();
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

  // Plain anchors (nav, PosterCard, ArrowLink) route in-app without a reload.
  const onClick = (e: MouseEvent<HTMLElement>) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey)
      return;
    const a = (e.target as HTMLElement).closest?.('a');
    const href = a?.getAttribute('href');
    if (
      !a ||
      !href ||
      a.target ||
      a.hasAttribute('download') ||
      !href.startsWith('/') ||
      href.startsWith('//')
    )
      return;
    e.preventDefault();
    router.history.push(href);
  };

  return (
    <QueryClientProvider client={client}>
      <ThemeProvider
        section="cinema"
        perSection={theme.perSection}
        appearance={theme.appearance}
        style={theme.style}
        {...theme.data}
        className="cin-root"
        onClick={onClick}
      >
        <ToastProvider>
          <AgeGateProvider>
            <Frame />
          </AgeGateProvider>
        </ToastProvider>
      </ThemeProvider>
    </QueryClientProvider>
  );
}

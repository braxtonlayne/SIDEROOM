import { ThemeProvider, ToastProvider } from '@sideroom/ui';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Outlet, useRouter, useRouterState } from '@tanstack/react-router';
import { useState, type MouseEvent } from 'react';
import { AgeGateProvider } from '../lib/ageGate';
import { useAppliedTheme } from '../settings/user/useAppliedTheme';
import { LibraryShell, useRequireSession } from './Shell';
import './library.css';

/** The section frame: own query cache, age gate, theme and chrome. Loaded lazily by routes.tsx. */
function Frame() {
  const reading = useRouterState({ select: (s) => s.location.pathname.startsWith('/library/r/') });
  const unauthorized = useRequireSession();
  if (unauthorized) return null;
  return reading ? <Outlet /> : <LibraryShell />;
}

export function LibraryRoot() {
  const theme = useAppliedTheme(undefined, 'library');
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

  // The shared UI kit renders plain anchors (PosterCard, ArrowLink): route in-app links without a reload.
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
        section="library"
        perSection={theme.perSection}
        appearance={theme.appearance}
        style={theme.style}
        {...theme.data}
        className="lib-root"
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

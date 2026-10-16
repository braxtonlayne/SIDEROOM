// Standalone frame for the player routes when they are mounted straight on the root route (no Cinema section
// layout around them): own query cache, theme, toasts and the "Are you 18 or older?" gate. When the Cinema
// section layout mounts `createPlayerRoutes`, it provides all of this and this frame is not used.
import { ThemeProvider, ToastProvider } from '@sideroom/ui';
import { QueryClient, QueryClientProvider, useQuery } from '@tanstack/react-query';
import { Outlet, useNavigate, useRouter, useRouterState } from '@tanstack/react-router';
import { useEffect, useRef, useState, type MouseEvent } from 'react';
import { ApiError, apiFetch } from '../../api/client';
import { AgeGateProvider } from '../../lib/ageGate';
import { useAppliedTheme } from '../../settings/user/useAppliedTheme';
import './player.css';

function Session({ children }: { children: React.ReactNode }) {
  const navigate = useNavigate();
  const here = useRouterState({ select: (s) => s.location.href });
  const sent = useRef(false);
  const me = useQuery({
    queryKey: ['cinema', 'me'],
    queryFn: () => apiFetch<{ id: string }>('/users/@me'),
    staleTime: 5 * 60_000,
    retry: false,
  });
  const unauthorized = me.error instanceof ApiError && me.error.status === 401;
  useEffect(() => {
    if (!unauthorized || sent.current) return;
    sent.current = true;
    void navigate({ to: '/login', search: { next: here } });
  }, [unauthorized, navigate, here]);
  return unauthorized ? null : <>{children}</>;
}

export function PlayerFrame() {
  const theme = useAppliedTheme(undefined, 'cinema');
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
  // The UI kit renders plain anchors: route in-app links without a reload.
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
        className="cpl-root"
        onClick={onClick}
      >
        <ToastProvider>
          <AgeGateProvider>
            <Session>
              <Outlet />
            </Session>
          </AgeGateProvider>
        </ToastProvider>
      </ThemeProvider>
    </QueryClientProvider>
  );
}

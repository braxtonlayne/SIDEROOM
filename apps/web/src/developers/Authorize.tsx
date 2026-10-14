import { Button, Notice, ThemeProvider, ToastProvider } from '@sideroom/ui';
import { QueryClient, QueryClientProvider, useMutation, useQuery } from '@tanstack/react-query';
import { Link, useNavigate, useRouter } from '@tanstack/react-router';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { ApiError } from '../api/client';
import { useAppliedTheme } from '../settings/user/useAppliedTheme';
import { errorText, previewAuthorize, submitAuthorize, type AuthorizeQuery } from './api';
import { AppIcon } from './Applications';
import { external, isSafeRedirect, parseScopes, permissionLabels, scopeSentence } from './oauth';
import type { AuthorizeSearch } from './search';
import type { ConsentGuild } from './types';
import { Loading } from './ui';
import './developers.css';

const REASON: Record<NonNullable<ConsentGuild['reason']>, string> = {
  missing_permission: 'you need Manage Server',
  app_not_released: 'test servers only',
  bot_not_public: 'private bot',
};

function Frame({ children }: { children: ReactNode }) {
  return (
    <main className="dev-authorize" id="dev-main">
      <div className="dev-auth-card">{children}</div>
    </main>
  );
}

function Problem({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Frame>
      <h1 className="dev-auth-title">{title}</h1>
      <p role="alert" className="dev-muted">
        {children}
      </p>
      <Link to="/channels" className="dev-link">
        Back to the lounge
      </Link>
    </Frame>
  );
}

function AuthorizeScreen({ search }: { search: AuthorizeSearch }) {
  const navigate = useNavigate();
  const router = useRouter();
  const q: AuthorizeQuery | null = search.client_id
    ? {
        client_id: search.client_id,
        scope: search.scope ?? '',
        ...(search.redirect_uri ? { redirect_uri: search.redirect_uri } : {}),
        ...(search.response_type ? { response_type: search.response_type } : {}),
        ...(search.state ? { state: search.state } : {}),
        ...(search.permissions ? { permissions: search.permissions } : {}),
        ...(search.code_challenge ? { code_challenge: search.code_challenge } : {}),
        ...(search.code_challenge_method
          ? { code_challenge_method: search.code_challenge_method }
          : {}),
      }
    : null;

  const preview = useQuery({
    queryKey: ['oauth2', 'preview', q],
    queryFn: () => previewAuthorize(q!),
    enabled: q !== null && parseScopes(q.scope).length > 0,
    retry: false,
  });
  const wantsBot = parseScopes(search.scope).includes('bot');
  const [guildId, setGuildId] = useState(search.guild_id ?? '');
  const [done, setDone] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const redirected = useRef(false);

  // Rebuilt from the parsed values: the router's own href JSON-quotes numeric-looking strings.
  const here = `/oauth2/authorize?${new URLSearchParams(search as Record<string, string>).toString()}`;
  const unauthorized = preview.error instanceof ApiError && preview.error.status === 401;
  useEffect(() => {
    if (!unauthorized || redirected.current) return;
    redirected.current = true;
    void navigate({ to: '/login', search: { next: here } });
  }, [unauthorized, navigate, here]);

  const follow = (location: string, redirectUri: string | null): boolean => {
    if (location.startsWith('/') && !location.startsWith('//')) {
      router.history.push(location);
      return true;
    }
    if (isSafeRedirect(location, redirectUri)) {
      external.go(location);
      return true;
    }
    return false;
  };

  const finish = useMutation({
    mutationFn: (authorize: boolean) =>
      submitAuthorize({ ...q!, authorize, ...(wantsBot && guildId ? { guild_id: guildId } : {}) }),
    onSuccess: (r, authorize) => {
      const target = preview.data?.redirect_uri ?? null;
      if (r.location) {
        if (!follow(r.location, target))
          setError(
            'The server sent you somewhere that is not a registered redirect, so we stopped.',
          );
        return;
      }
      if (authorize) setDone(wantsBot ? 'The bot was added to your server.' : 'Access granted.');
      else void navigate({ to: '/channels' });
    },
    onError: (e) => setError(errorText(e, 'Could not complete the authorization.')),
  });

  if (!q) return <Problem title="Cannot authorize">This link is missing a client_id.</Problem>;
  if (parseScopes(q.scope).length === 0)
    return <Problem title="Cannot authorize">This link does not ask for any scope.</Problem>;
  if (preview.isPending || unauthorized)
    return (
      <Frame>
        <Loading what="the authorization request" />
      </Frame>
    );
  if (preview.error) {
    const e = preview.error;
    return (
      <Problem title="This request cannot be authorized">
        {e instanceof ApiError && e.status === 404
          ? 'We do not know that application.'
          : errorText(e, 'The request is not valid.')}{' '}
        Nothing was shared. Go back to the app and try again.
      </Problem>
    );
  }

  const p = preview.data;
  if (done)
    return (
      <Frame>
        <h1 className="dev-auth-title">Authorized</h1>
        <p role="status">{done}</p>
        <Link to="/channels" className="dev-link">
          Back to the lounge
        </Link>
      </Frame>
    );

  const labels = permissionLabels(p.permissions);
  const adminAsked = labels.includes('Administrator');
  const needGuild = wantsBot && !guildId;
  const redirectHost = (() => {
    try {
      return p.redirect_uri ? new URL(p.redirect_uri).host : null;
    } catch {
      return null;
    }
  })();
  const who = p.user.global_name || p.user.username;

  return (
    <Frame>
      <div className="dev-auth-head">
        <AppIcon app={p.application} size={64} />
        <h1 className="dev-auth-title">{p.application.name}</h1>
        <p className="dev-muted">wants to access the SIDE ROOM account {who}</p>
      </div>
      {p.application.release_state !== 'released' && (
        <Notice variant="warning" title="This app is still in development">
          It has not been through app review. Only continue if you know the developer.
        </Notice>
      )}
      {p.already_authorized && (
        <p className="dev-muted">You have authorized this app before. Continuing refreshes it.</p>
      )}
      {p.application.description && <p className="dev-muted">{p.application.description}</p>}
      <h2 className="dev-auth-sub">This will allow it to</h2>
      <ul className="dev-scopes" aria-label="Requested access">
        {p.scopes.map((s) => {
          const sentence = scopeSentence(s.name);
          return <li key={s.name}>{sentence === s.name ? s.description || s.name : sentence}</li>;
        })}
      </ul>

      {wantsBot && (
        <>
          <div className="dev-field">
            <label className="dev-label" htmlFor="dev-auth-guild">
              Add to server
            </label>
            <select
              id="dev-auth-guild"
              className="dev-select"
              value={guildId}
              onChange={(e) => setGuildId(e.target.value)}
            >
              <option value="">Select a server</option>
              {p.guilds.map((g) => (
                <option key={g.id} value={g.id} disabled={!g.can_install}>
                  {g.can_install
                    ? g.name
                    : `${g.name} (${g.reason ? REASON[g.reason] : 'cannot add'})`}
                </option>
              ))}
            </select>
            {!p.guilds.some((g) => g.can_install) && (
              <p className="dev-muted">
                There is no server you can add this bot to. You need Manage Server
                {p.application.release_state !== 'released' &&
                  ', and until the app is released it only works in your own small test servers'}
                .
              </p>
            )}
          </div>
          {labels.length > 0 && (
            <>
              <h2 className="dev-auth-sub">The bot will be able to</h2>
              <ul className="dev-scopes" aria-label="Bot permissions">
                {labels.map((l) => (
                  <li key={l}>{l}</li>
                ))}
              </ul>
              <p className="dev-muted">
                It only gets the permissions you hold yourself in that server.
              </p>
              {adminAsked && (
                <Notice variant="warning" title="Administrator">
                  Administrator lets the bot do anything in the server and ignores channel limits.
                </Notice>
              )}
            </>
          )}
        </>
      )}

      {redirectHost && (
        <p className="dev-muted">
          You will be sent to <strong>{redirectHost}</strong> afterwards.
        </p>
      )}
      {error && (
        <p className="dev-error" role="alert">
          {error}
        </p>
      )}
      <div className="dev-auth-actions">
        <Button variant="ghost" disabled={finish.isPending} onClick={() => finish.mutate(false)}>
          Cancel
        </Button>
        <Button
          loading={finish.isPending}
          disabled={needGuild}
          onClick={() => {
            setError(null);
            finish.mutate(true);
          }}
        >
          Authorize
        </Button>
      </div>
    </Frame>
  );
}

function Providers({ children }: { children: ReactNode }) {
  const theme = useAppliedTheme();
  const [client] = useState(
    () =>
      new QueryClient({
        defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } },
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
        <ToastProvider>{children}</ToastProvider>
      </ThemeProvider>
    </QueryClientProvider>
  );
}

export function AuthorizePage({ search }: { search: AuthorizeSearch }) {
  return (
    <Providers>
      <AuthorizeScreen search={search} />
    </Providers>
  );
}

/** /oauth2/authorized: where an approval without a redirect URI lands (a bot install, for example). */
export function AuthorizedPage() {
  return (
    <Providers>
      <Frame>
        <h1 className="dev-auth-title">Authorized</h1>
        <p role="status">You can close this tab and go back to the app.</p>
        <Link to="/channels" className="dev-link">
          Back to the lounge
        </Link>
      </Frame>
    </Providers>
  );
}

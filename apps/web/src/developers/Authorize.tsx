import { Button, Notice, ThemeProvider, ToastProvider } from '@sideroom/ui';
import { QueryClient, QueryClientProvider, useMutation, useQuery } from '@tanstack/react-query';
import { Link, useNavigate, useRouter } from '@tanstack/react-router';
import { useEffect, useRef, useState, type ReactNode } from 'react';
import { ApiError } from '../api/client';
import { useT, type MessageKey } from '../i18n';
import { useAppliedTheme } from '../settings/user/useAppliedTheme';
import { errorText, previewAuthorize, submitAuthorize, type AuthorizeQuery } from './api';
import { AppIcon } from './Applications';
import { external, isSafeRedirect, parseScopes, permissionLabels, scopeSentence } from './oauth';
import type { AuthorizeSearch } from './search';
import type { ConsentGuild } from './types';
import { Loading } from './ui';
import './developers.css';

const REASON: Record<NonNullable<ConsentGuild['reason']>, MessageKey> = {
  missing_permission: 'developers.authorize.reasonMissingPermission',
  app_not_released: 'developers.authorize.reasonAppNotReleased',
  bot_not_public: 'developers.authorize.reasonBotNotPublic',
};

function Frame({ children }: { children: ReactNode }) {
  return (
    <main className="dev-authorize" id="dev-main">
      <div className="dev-auth-card">{children}</div>
    </main>
  );
}

function Problem({ title, children }: { title: string; children: ReactNode }) {
  const t = useT();
  return (
    <Frame>
      <h1 className="dev-auth-title">{title}</h1>
      <p role="alert" className="dev-muted">
        {children}
      </p>
      <Link to="/channels" className="dev-link">
        {t('developers.authorize.backToLounge')}
      </Link>
    </Frame>
  );
}

function AuthorizeScreen({ search }: { search: AuthorizeSearch }) {
  const t = useT();
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
        if (!follow(r.location, target)) setError(t('developers.authorize.badRedirect'));
        return;
      }
      if (authorize)
        setDone(
          wantsBot ? t('developers.authorize.botAdded') : t('developers.authorize.accessGranted'),
        );
      else void navigate({ to: '/channels' });
    },
    onError: (e) => setError(errorText(e, t('developers.authorize.failed'))),
  });

  if (!q)
    return (
      <Problem title={t('developers.authorize.cannotAuthorize')}>
        {t('developers.authorize.missingClientId')}
      </Problem>
    );
  if (parseScopes(q.scope).length === 0)
    return (
      <Problem title={t('developers.authorize.cannotAuthorize')}>
        {t('developers.authorize.noScope')}
      </Problem>
    );
  if (preview.isPending || unauthorized)
    return (
      <Frame>
        <Loading what={t('developers.authorize.theRequest')} />
      </Frame>
    );
  if (preview.error) {
    const e = preview.error;
    return (
      <Problem title={t('developers.authorize.requestInvalidTitle')}>
        {e instanceof ApiError && e.status === 404
          ? t('developers.authorize.unknownApp')
          : errorText(e, t('developers.authorize.requestNotValid'))}{' '}
        {t('developers.authorize.nothingShared')}
      </Problem>
    );
  }

  const p = preview.data;
  if (done)
    return (
      <Frame>
        <h1 className="dev-auth-title">{t('developers.authorize.authorized')}</h1>
        <p role="status">{done}</p>
        <Link to="/channels" className="dev-link">
          {t('developers.authorize.backToLounge')}
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
        <p className="dev-muted">{t('developers.authorize.wantsAccess', { who })}</p>
      </div>
      {p.application.release_state !== 'released' && (
        <Notice variant="warning" title={t('developers.authorize.devTitle')}>
          {t('developers.authorize.devBody')}
        </Notice>
      )}
      {p.already_authorized && (
        <p className="dev-muted">{t('developers.authorize.alreadyAuthorized')}</p>
      )}
      {p.application.description && <p className="dev-muted">{p.application.description}</p>}
      <h2 className="dev-auth-sub">{t('developers.authorize.willAllow')}</h2>
      <ul className="dev-scopes" aria-label={t('developers.authorize.requestedAccess')}>
        {p.scopes.map((s) => {
          const sentence = scopeSentence(s.name);
          return <li key={s.name}>{sentence === s.name ? s.description || s.name : sentence}</li>;
        })}
      </ul>

      {wantsBot && (
        <>
          <div className="dev-field">
            <label className="dev-label" htmlFor="dev-auth-guild">
              {t('developers.authorize.addToServer')}
            </label>
            <select
              id="dev-auth-guild"
              className="dev-select"
              value={guildId}
              onChange={(e) => setGuildId(e.target.value)}
            >
              <option value="">{t('developers.authorize.selectServer')}</option>
              {p.guilds.map((g) => (
                <option key={g.id} value={g.id} disabled={!g.can_install}>
                  {g.can_install
                    ? g.name
                    : t('developers.authorize.guildOption', {
                        name: g.name,
                        reason: g.reason
                          ? t(REASON[g.reason])
                          : t('developers.authorize.cannotAdd'),
                      })}
                </option>
              ))}
            </select>
            {!p.guilds.some((g) => g.can_install) && (
              <p className="dev-muted">
                {p.application.release_state !== 'released'
                  ? t('developers.authorize.noServerDev')
                  : t('developers.authorize.noServer')}
              </p>
            )}
          </div>
          {labels.length > 0 && (
            <>
              <h2 className="dev-auth-sub">{t('developers.authorize.botCan')}</h2>
              <ul className="dev-scopes" aria-label={t('developers.authorize.botPermissions')}>
                {labels.map((l) => (
                  <li key={l}>{l}</li>
                ))}
              </ul>
              <p className="dev-muted">{t('developers.authorize.botOnlyGets')}</p>
              {adminAsked && (
                <Notice variant="warning" title={t('developers.authorize.adminTitle')}>
                  {t('developers.authorize.adminBody')}
                </Notice>
              )}
            </>
          )}
        </>
      )}

      {redirectHost && (
        <p className="dev-muted">
          {t('developers.authorize.sentTo')} <strong>{redirectHost}</strong>{' '}
          {t('developers.authorize.afterwards')}
        </p>
      )}
      {error && (
        <p className="dev-error" role="alert">
          {error}
        </p>
      )}
      <div className="dev-auth-actions">
        <Button variant="ghost" disabled={finish.isPending} onClick={() => finish.mutate(false)}>
          {t('developers.authorize.cancel')}
        </Button>
        <Button
          loading={finish.isPending}
          disabled={needGuild}
          onClick={() => {
            setError(null);
            finish.mutate(true);
          }}
        >
          {t('developers.authorize.authorize')}
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
  const t = useT();
  return (
    <Providers>
      <Frame>
        <h1 className="dev-auth-title">{t('developers.authorize.authorized')}</h1>
        <p role="status">{t('developers.authorize.closeTab')}</p>
        <Link to="/channels" className="dev-link">
          {t('developers.authorize.backToLounge')}
        </Link>
      </Frame>
    </Providers>
  );
}

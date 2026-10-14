import {
  Button,
  Checkbox,
  Notice,
  PermissionMatrix,
  TextInput,
  useToast,
  type TriState,
} from '@sideroom/ui';
import { useId, useMemo, useState } from 'react';
import { bitsToValues, permissionGroups, toBig } from '../../settings/server/permissions';
import { errorText, resetClientSecret } from '../api';
import { useT } from '../../i18n';
import { usePatchApp } from '../hooks';
import {
  buildAuthorizeUrl,
  MAX_REDIRECTS,
  permissionBits,
  redirectUriError,
  SCOPES,
} from '../oauth';
import type { DevApp } from '../types';
import { CopyButton, PasswordConfirm, SecretDialog } from '../ui';

function ClientSecret({ app }: { app: DevApp }) {
  const t = useT();
  const [confirm, setConfirm] = useState(false);
  // Held only while the reveal dialog is open.
  const [secret, setSecret] = useState<string | null>(null);

  return (
    <>
      <div className="dev-field">
        <span className="dev-label">{t('developers.oauth2.clientSecret')}</span>
        <p className="dev-muted">
          {app.client_secret_hint
            ? t('developers.oauth2.secretSet', {
                hint: app.client_secret_hint.replace(/^[.…]+/, ''),
              })
            : t('developers.oauth2.secretNotSet')}
        </p>
        <div className="dev-actions">
          <Button variant="secondary" onClick={() => setConfirm(true)}>
            {t('developers.oauth2.resetSecret')}
          </Button>
        </div>
      </div>
      <PasswordConfirm
        open={confirm}
        title={t('developers.oauth2.resetTitle')}
        description={t('developers.oauth2.resetDescription')}
        confirmLabel={t('developers.oauth2.resetConfirm')}
        onCancel={() => setConfirm(false)}
        onSubmit={async (password) => {
          const r = await resetClientSecret(app.id, password);
          setConfirm(false);
          setSecret(r.client_secret);
        }}
      />
      <SecretDialog
        title={t('developers.oauth2.newSecretTitle')}
        description={t('developers.oauth2.newSecretDescription')}
        secrets={
          secret === null ? null : [{ label: t('developers.oauth2.clientSecret'), value: secret }]
        }
        onClose={() => setSecret(null)}
      />
    </>
  );
}

function Redirects({ app }: { app: DevApp }) {
  const t = useT();
  const patch = usePatchApp(app.id);
  const [uris, setUris] = useState(app.redirect_uris);
  const [draft, setDraft] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const dirty = uris.join('\n') !== app.redirect_uris.join('\n');

  const add = () => {
    const value = draft.trim();
    const problem =
      redirectUriError(value) ??
      (uris.includes(value) ? t('developers.oauth2.uriDuplicate') : null) ??
      (uris.length >= MAX_REDIRECTS ? t('developers.oauth2.uriMax', { max: MAX_REDIRECTS }) : null);
    if (problem) {
      setError(problem);
      return;
    }
    setError(null);
    setUris([...uris, value]);
    setDraft('');
  };

  return (
    <section className="dev-card" aria-labelledby="dev-redirects-h">
      <h2 id="dev-redirects-h" className="dev-card-title">
        {t('developers.oauth2.redirectsHeading')}
      </h2>
      <p className="dev-muted">{t('developers.oauth2.redirectsIntro')}</p>
      {uris.length === 0 ? (
        <p className="dev-muted">{t('developers.oauth2.noRedirects')}</p>
      ) : (
        <ul className="dev-uri-list" aria-label={t('developers.oauth2.redirectsLabel')}>
          {uris.map((u) => (
            <li key={u}>
              <code>{u}</code>
              <Button
                variant="ghost"
                size="sm"
                aria-label={t('developers.oauth2.removeUri', { uri: u })}
                onClick={() => setUris(uris.filter((x) => x !== u))}
              >
                {t('developers.oauth2.remove')}
              </Button>
            </li>
          ))}
        </ul>
      )}
      <form
        className="dev-inline-form"
        onSubmit={(e) => {
          e.preventDefault();
          add();
        }}
      >
        <TextInput
          label={t('developers.oauth2.addUriLabel')}
          placeholder={t('developers.oauth2.addUriPlaceholder')}
          value={draft}
          onChange={(e) => {
            setDraft(e.target.value);
            setError(null);
          }}
          error={error ?? undefined}
          autoComplete="off"
          inputMode="url"
        />
        <Button variant="secondary" type="submit">
          {t('developers.oauth2.add')}
        </Button>
      </form>
      {saveError && (
        <p className="dev-error" role="alert">
          {saveError}
        </p>
      )}
      <div className="dev-actions">
        <Button
          disabled={!dirty}
          loading={patch.isPending}
          onClick={() => {
            setSaveError(null);
            patch.mutate(
              { redirect_uris: uris },
              {
                onError: (e) =>
                  setSaveError(errorText(e, t('developers.oauth2.saveRedirectsFailed'))),
              },
            );
          }}
        >
          {t('developers.oauth2.saveRedirects')}
        </Button>
      </div>
    </section>
  );
}

/** Scope picker, permission calculator and the generated authorize URL. */
export function UrlGenerator({ app, installOnly = false }: { app: DevApp; installOnly?: boolean }) {
  const t = useT();
  const toast = useToast();
  const patch = usePatchApp(app.id);
  const [scopes, setScopes] = useState<string[]>(installOnly ? app.install_params.scopes : []);
  const [redirect, setRedirect] = useState<string>('');
  const [perms, setPerms] = useState<Record<string, TriState>>(
    installOnly ? bitsToValues(toBig(app.install_params.permissions)) : {},
  );
  const [saveError, setSaveError] = useState<string | null>(null);
  const groups = useMemo(() => permissionGroups(), []);
  const bitsId = useId();

  const bits = permissionBits(
    Object.entries(perms)
      .filter(([, v]) => v === 'allow')
      .map(([k]) => k),
  );
  const wantsBot = scopes.includes('bot');
  const url = buildAuthorizeUrl({
    origin: window.location.origin,
    clientId: app.id,
    scopes,
    ...(redirect && !installOnly ? { redirectUri: redirect } : {}),
    permissions: bits,
  });

  const toggle = (id: string, on: boolean) =>
    setScopes((cur) => (on ? [...cur, id] : cur.filter((s) => s !== id)));

  return (
    <section className="dev-card" aria-labelledby={`${bitsId}-h`}>
      <h2 id={`${bitsId}-h`} className="dev-card-title">
        {installOnly ? t('developers.oauth2.inviteLink') : t('developers.oauth2.urlGenerator')}
      </h2>
      <fieldset className="dev-fieldset">
        <legend>{t('developers.oauth2.scopes')}</legend>
        <div className="dev-scope-grid">
          {SCOPES.filter(
            (s) => !installOnly || s.id === 'bot' || s.id === 'applications.commands',
          ).map((s) => (
            <Checkbox
              key={s.id}
              label={s.label}
              description={s.description}
              checked={scopes.includes(s.id)}
              onChange={(e) => toggle(s.id, e.target.checked)}
            />
          ))}
        </div>
      </fieldset>

      {!installOnly && (
        <div className="dev-field">
          <label className="dev-label" htmlFor={`${bitsId}-redirect`}>
            {t('developers.oauth2.redirectUri')}
          </label>
          <select
            id={`${bitsId}-redirect`}
            className="dev-select"
            value={redirect}
            onChange={(e) => setRedirect(e.target.value)}
          >
            <option value="">{t('developers.oauth2.noneInstallOnly')}</option>
            {app.redirect_uris.map((u) => (
              <option key={u} value={u}>
                {u}
              </option>
            ))}
          </select>
        </div>
      )}

      {wantsBot && (
        <div className="dev-field">
          <span className="dev-label">{t('developers.oauth2.botPermissions')}</span>
          <PermissionMatrix
            groups={groups}
            values={perms}
            onChange={(id, v) => setPerms((cur) => ({ ...cur, [id]: v }))}
            searchLabel={t('developers.oauth2.searchPermissions')}
          />
          <p className="dev-muted" id={bitsId}>
            {t('developers.oauth2.permissionsInteger')} <code>{bits.toString()}</code>
          </p>
          <p className="dev-muted">{t('developers.oauth2.botGets')}</p>
          {perms.ADMINISTRATOR === 'allow' && (
            <Notice variant="warning" title={t('developers.oauth2.adminTitle')}>
              {t('developers.oauth2.adminBody')}
            </Notice>
          )}
        </div>
      )}

      <div className="dev-field">
        <TextInput
          label={t('developers.oauth2.generatedUrl')}
          value={scopes.length ? url : ''}
          readOnly
          placeholder={t('developers.oauth2.pickScope')}
        />
        <div className="dev-actions">
          <CopyButton text={url} label={t('developers.oauth2.copyUrl')} size="md" />
          {installOnly && (
            <Button
              variant="secondary"
              loading={patch.isPending}
              onClick={() => {
                setSaveError(null);
                patch.mutate(
                  { install_params: { scopes, permissions: bits.toString() } },
                  {
                    onSuccess: () =>
                      toast.show({
                        title: t('developers.oauth2.defaultsSaved'),
                        variant: 'success',
                      }),
                    onError: (e) =>
                      setSaveError(errorText(e, t('developers.oauth2.saveDefaultsFailed'))),
                  },
                );
              }}
            >
              {t('developers.oauth2.saveDefaults')}
            </Button>
          )}
        </div>
        {saveError && (
          <p className="dev-error" role="alert">
            {saveError}
          </p>
        )}
      </div>
    </section>
  );
}

export function OAuth2Tab({ app }: { app: DevApp }) {
  const t = useT();
  return (
    <div className="dev-stack">
      <section className="dev-card" aria-labelledby="dev-client-h">
        <h2 id="dev-client-h" className="dev-card-title">
          {t('developers.oauth2.clientInfo')}
        </h2>
        <div className="dev-field">
          <TextInput label={t('developers.oauth2.clientId')} value={app.id} readOnly />
          <div className="dev-actions">
            <CopyButton text={app.id} label={t('developers.oauth2.copyClientId')} />
          </div>
        </div>
        <ClientSecret app={app} />
      </section>
      <Redirects key={app.redirect_uris.join('|')} app={app} />
      <UrlGenerator app={app} />
    </div>
  );
}

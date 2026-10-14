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
  const [confirm, setConfirm] = useState(false);
  // Held only while the reveal dialog is open.
  const [secret, setSecret] = useState<string | null>(null);

  return (
    <>
      <div className="dev-field">
        <span className="dev-label">Client secret</span>
        <p className="dev-muted">
          {app.client_secret_hint
            ? `A secret ending in ${app.client_secret_hint.replace(/^[.…]+/, '')} is set. It is stored hashed, so it cannot be shown again.`
            : 'No secret is set.'}
        </p>
        <div className="dev-actions">
          <Button variant="secondary" onClick={() => setConfirm(true)}>
            Reset secret
          </Button>
        </div>
      </div>
      <PasswordConfirm
        open={confirm}
        title="Reset the client secret?"
        description="The current secret stops working immediately. Anything using it will fail until you update it. Confirm with your password."
        confirmLabel="Yes, reset it"
        onCancel={() => setConfirm(false)}
        onSubmit={async (password) => {
          const r = await resetClientSecret(app.id, password);
          setConfirm(false);
          setSecret(r.client_secret);
        }}
      />
      <SecretDialog
        title="New client secret"
        description="Use it as client_secret when exchanging an authorization code."
        secrets={secret === null ? null : [{ label: 'Client secret', value: secret }]}
        onClose={() => setSecret(null)}
      />
    </>
  );
}

function Redirects({ app }: { app: DevApp }) {
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
      (uris.includes(value) ? 'That URI is already in the list.' : null) ??
      (uris.length >= MAX_REDIRECTS
        ? `You can register up to ${MAX_REDIRECTS} redirect URIs.`
        : null);
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
        Redirects
      </h2>
      <p className="dev-muted">
        After someone authorizes, we send them to one of these exact URLs. Anything else is refused.
        Use https, or http on localhost.
      </p>
      {uris.length === 0 ? (
        <p className="dev-muted">No redirect URIs yet.</p>
      ) : (
        <ul className="dev-uri-list" aria-label="Redirect URIs">
          {uris.map((u) => (
            <li key={u}>
              <code>{u}</code>
              <Button
                variant="ghost"
                size="sm"
                aria-label={`Remove ${u}`}
                onClick={() => setUris(uris.filter((x) => x !== u))}
              >
                Remove
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
          label="Add a redirect URI"
          placeholder="https://example.com/callback"
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
          Add
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
              { onError: (e) => setSaveError(errorText(e, 'Could not save the redirects.')) },
            );
          }}
        >
          Save redirects
        </Button>
      </div>
    </section>
  );
}

/** Scope picker, permission calculator and the generated authorize URL. */
export function UrlGenerator({ app, installOnly = false }: { app: DevApp; installOnly?: boolean }) {
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
        {installOnly ? 'Invite link' : 'URL generator'}
      </h2>
      <fieldset className="dev-fieldset">
        <legend>Scopes</legend>
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
            Redirect URI
          </label>
          <select
            id={`${bitsId}-redirect`}
            className="dev-select"
            value={redirect}
            onChange={(e) => setRedirect(e.target.value)}
          >
            <option value="">None (install only)</option>
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
          <span className="dev-label">Bot permissions</span>
          <PermissionMatrix
            groups={groups}
            values={perms}
            onChange={(id, v) => setPerms((cur) => ({ ...cur, [id]: v }))}
            searchLabel="Search bot permissions"
          />
          <p className="dev-muted" id={bitsId}>
            Permissions integer: <code>{bits.toString()}</code>
          </p>
          <p className="dev-muted">
            The bot only gets what you ask for and the person adding it holds themselves.
          </p>
          {perms.ADMINISTRATOR === 'allow' && (
            <Notice variant="warning" title="Administrator bypasses every permission check">
              Only ask for it if your bot truly needs it. People may refuse to install it.
            </Notice>
          )}
        </div>
      )}

      <div className="dev-field">
        <TextInput
          label="Generated URL"
          value={scopes.length ? url : ''}
          readOnly
          placeholder="Pick at least one scope"
        />
        <div className="dev-actions">
          <CopyButton text={url} label="Copy URL" size="md" />
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
                      toast.show({ title: 'Default install settings saved.', variant: 'success' }),
                    onError: (e) => setSaveError(errorText(e, 'Could not save the defaults.')),
                  },
                );
              }}
            >
              Save as default install settings
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
  return (
    <div className="dev-stack">
      <section className="dev-card" aria-labelledby="dev-client-h">
        <h2 id="dev-client-h" className="dev-card-title">
          Client information
        </h2>
        <div className="dev-field">
          <TextInput label="Client ID" value={app.id} readOnly />
          <div className="dev-actions">
            <CopyButton text={app.id} label="Copy client ID" />
          </div>
        </div>
        <ClientSecret app={app} />
      </section>
      <Redirects key={app.redirect_uris.join('|')} app={app} />
      <UrlGenerator app={app} />
    </div>
  );
}

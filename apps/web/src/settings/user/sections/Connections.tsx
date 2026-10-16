import { Button, Notice, Switch, TextInput, useToast } from '@sideroom/ui';
import { useState } from 'react';
import { useT, type MessageKey } from '../../../i18n';
import {
  authorizeConnection,
  errorText,
  listConnections,
  patchConnection,
  putConnectionLink,
  refreshConnection,
  removeConnection,
  useResource,
  type ConnectionProvider,
  type ConnectionRow,
  type ConnectionsResponse,
  type Resource,
} from '../api';
import { Card, Loaded, SectionTitle } from '../parts';

/** The OAuth return lands on `?connected=<provider>` or `?error=<code>` (LNG-USET-023). */
function useReturnNotice(): { connected?: string; error?: string } {
  const [params] = useState(() => {
    try {
      return new URLSearchParams(window.location.search);
    } catch {
      return new URLSearchParams();
    }
  });
  return {
    connected: params.get('connected') ?? undefined,
    error: params.get('error') ?? undefined,
  };
}

/** Return error codes of the callback -> catalog key. Anything else reads as a generic failure. */
const RETURN_ERRORS: Record<string, string> = {
  denied: 'settings.connections.errorDenied',
  state: 'settings.connections.errorState',
  exchange: 'settings.connections.errorExchange',
  profile: 'settings.connections.errorProfile',
  taken: 'settings.connections.errorTaken',
  limit: 'settings.connections.errorLimit',
  signed_out: 'settings.connections.errorSignedOut',
};

function ConnectedItem({
  row,
  onChange,
  onRemoved,
}: {
  row: ConnectionRow;
  onChange: (r: ConnectionRow) => void;
  onRemoved: () => void;
}) {
  const t = useT();
  const toast = useToast();
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const run = async (fn: () => Promise<void>) => {
    setError(null);
    setBusy(true);
    try {
      await fn();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };
  const stats = Object.entries(row.stats).filter(
    ([, v]) => typeof v === 'number' || typeof v === 'string',
  );
  return (
    <li>
      <div className="us-stack">
        <div className="us-row-main">
          {row.url ? (
            <a href={row.url} target="_blank" rel="noopener noreferrer nofollow">
              <strong>{row.name}</strong>
            </a>
          ) : (
            <strong>{row.name}</strong>
          )}{' '}
          <span className="us-muted">{row.type}</span>
          <span className="us-badge" data-tone={row.verified ? 'good' : undefined}>
            {row.verified
              ? t('settings.connections.verified')
              : t('settings.connections.unverified')}
          </span>
        </div>
        {row.show_activity && stats.length > 0 && (
          <p className="us-muted">{stats.map(([k, v]) => `${k}: ${String(v)}`).join(' · ')}</p>
        )}
        <Switch
          label={t('settings.connections.showOnProfile')}
          checked={row.visible}
          disabled={busy}
          onChange={(e) => {
            const visible = e.target.checked;
            void run(async () => onChange(await patchConnection(row, { visible })));
          }}
        />
        <Switch
          label={t('settings.connections.showStats')}
          checked={row.show_activity}
          disabled={busy}
          onChange={(e) => {
            const show = e.target.checked;
            void run(async () => onChange(await patchConnection(row, { show_activity: show })));
          }}
        />
        <div className="us-actions">
          {row.verified && (
            <Button
              variant="secondary"
              size="sm"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  onChange(await refreshConnection(row));
                  toast.show({ title: t('settings.connections.refreshed'), duration: 1500 });
                })
              }
            >
              {t('settings.connections.refresh')}
              <span className="us-sr"> {row.name}</span>
            </Button>
          )}
          <Button
            variant="danger"
            size="sm"
            disabled={busy}
            onClick={() =>
              void run(async () => {
                await removeConnection(row);
                onRemoved();
              })
            }
          >
            {t('settings.connections.remove')}
            <span className="us-sr"> {row.name}</span>
          </Button>
        </div>
        {error && (
          <Notice variant="danger" title={t('settings.connections.actionError')}>
            {error}
          </Notice>
        )}
      </div>
    </li>
  );
}

function LinkForm({
  provider,
  onDone,
  onCancel,
}: {
  provider: ConnectionProvider;
  onDone: () => void;
  onCancel: () => void;
}) {
  const t = useT();
  const [name, setName] = useState('');
  const [url, setUrl] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  return (
    <form
      className="us-stack"
      noValidate
      onSubmit={(e) => {
        e.preventDefault();
        setError(null);
        setBusy(true);
        putConnectionLink({
          provider: provider.id,
          name: name.trim() || undefined,
          url: url.trim() || undefined,
        })
          .then(onDone, (err: unknown) => setError(errorText(err)))
          .finally(() => setBusy(false));
      }}
    >
      <TextInput
        label={t('settings.connections.linkName', { provider: provider.name })}
        value={name}
        maxLength={100}
        onChange={(e) => setName(e.target.value)}
      />
      {provider.requires_url && (
        <TextInput
          label={t('settings.connections.linkUrl')}
          type="url"
          placeholder="https://"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
        />
      )}
      {error && (
        <Notice variant="danger" title={t('settings.connections.actionError')}>
          {error}
        </Notice>
      )}
      <div className="us-actions">
        <Button type="submit" disabled={busy}>
          {t('settings.connections.linkSubmit')}
        </Button>
        <Button variant="secondary" type="button" onClick={onCancel}>
          {t('settings.connections.linkCancel')}
        </Button>
      </div>
    </form>
  );
}

function ProvidersCard({ data, reload }: { data: ConnectionsResponse; reload: () => void }) {
  const t = useT();
  const [linking, setLinking] = useState<ConnectionProvider | null>(null);
  const [error, setError] = useState<string | null>(null);
  const connect = (p: ConnectionProvider) => {
    setError(null);
    if (!p.verified) {
      setLinking(p);
      return;
    }
    authorizeConnection(p.id).then(
      (r) => window.location.assign(r.url),
      (e: unknown) => setError(errorText(e)),
    );
  };
  return (
    <Card
      id="us-add-connection"
      title={t('settings.connections.addTitle')}
      description={t('settings.connections.addDesc')}
    >
      <ul className="us-providers" aria-label={t('settings.connections.providers')}>
        {data.providers.map((p) => (
          <li key={p.id}>
            <span>{p.name}</span>{' '}
            {p.available ? (
              <Button size="sm" variant="secondary" onClick={() => connect(p)}>
                {t('settings.connections.connect')}
                <span className="us-sr"> {p.name}</span>
              </Button>
            ) : (
              <span className="us-muted">{t('settings.connections.notSetUp')}</span>
            )}
          </li>
        ))}
      </ul>
      {linking && (
        <LinkForm
          provider={linking}
          onCancel={() => setLinking(null)}
          onDone={() => {
            setLinking(null);
            reload();
          }}
        />
      )}
      {error && (
        <Notice variant="danger" title={t('settings.connections.actionError')}>
          {error}
        </Notice>
      )}
    </Card>
  );
}

function patchRow(res: Resource<ConnectionsResponse>, next: ConnectionRow) {
  res.setData((d) =>
    d
      ? {
          ...d,
          connections: d.connections.map((c) =>
            c.type === next.type && c.id === next.id ? next : c,
          ),
        }
      : d,
  );
}

/** Settings > Connections: linked accounts (profile / stats toggles, refresh, remove) and the provider grid. */
export function ConnectionsSection() {
  const t = useT();
  const res = useResource(listConnections);
  const back = useReturnNotice();
  return (
    <>
      <SectionTitle>{t('settings.connections.title')}</SectionTitle>
      {back.connected && (
        <Notice
          variant="success"
          title={t('settings.connections.returnedOk', { provider: back.connected })}
        />
      )}
      {back.error && (
        <Notice variant="danger" title={t('settings.connections.returnedError')}>
          {t((RETURN_ERRORS[back.error] ?? 'settings.connections.errorFailed') as MessageKey)}
        </Notice>
      )}
      <Card id="us-connected" title={t('settings.connections.connected')}>
        <Loaded res={res} what={t('settings.connections.what')}>
          {(data) =>
            data.connections.length === 0 ? (
              <p className="us-muted">{t('settings.connections.none')}</p>
            ) : (
              <ul className="us-list">
                {data.connections.map((c) => (
                  <ConnectedItem
                    key={c.type + c.id}
                    row={c}
                    onChange={(next) => patchRow(res, next)}
                    onRemoved={res.reload}
                  />
                ))}
              </ul>
            )
          }
        </Loaded>
      </Card>
      {res.data && <ProvidersCard data={res.data} reload={res.reload} />}
    </>
  );
}

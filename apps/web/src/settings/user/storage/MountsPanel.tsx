import { Button, Notice, Select, TextInput } from '@sideroom/ui';
import { Link } from '@tanstack/react-router';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { errorText, useResource } from '../api';
import { Card, fmtDate, Loaded } from '../parts';
import { policyExample } from './policy';
import {
  applyCors,
  checkCors,
  createMount,
  deleteMount,
  listMounts,
  patchMount,
  testMount,
  type MountScope,
} from './api';
import {
  MOUNT_PROVIDERS,
  type CorsResult,
  type Mount,
  type MountProvider,
  type TestResult,
} from './types';
import './storage.css';

/** Per-provider form defaults (BYOS-MNT-001: R2 region auto and virtual-host style; the rest path-style). */
const PROVIDER_DEFAULTS: Record<MountProvider, { region: string; pathStyle: boolean }> = {
  s3: { region: '', pathStyle: false },
  r2: { region: 'auto', pathStyle: false },
  b2: { region: '', pathStyle: false },
  wasabi: { region: '', pathStyle: true },
  minio: { region: 'us-east-1', pathStyle: true },
  garage: { region: 'garage', pathStyle: true },
};

/**
 * Connect screen and mount list for one scope (the caller's own mounts, or a guild's). Used by
 * User Settings > Storage and Guild Settings > Storage (BYOS-UI-001). The secret key is write-only:
 * it is sent once and the list shows only the access key id and the last four characters.
 */
export function MountsPanel({ scope }: { scope: MountScope }) {
  const t = useT();
  const res = useResource(() => listMounts(scope));
  const [adding, setAdding] = useState(false);
  return (
    <Loaded res={res} what={t('settings.storage.what')}>
      {(mounts) => (
        <>
          <Card
            title={t('settings.storage.listTitle')}
            description={t('settings.storage.listDescription')}
            actions={
              !adding && (
                <Button onClick={() => setAdding(true)}>{t('settings.storage.connect')}</Button>
              )
            }
          >
            {mounts.length === 0 && !adding && (
              <p className="us-muted">{t('settings.storage.empty')}</p>
            )}
            <ul className="sto-list">
              {mounts.map((m) => (
                <li key={m.id}>
                  <MountRow scope={scope} mount={m} onChanged={res.reload} />
                </li>
              ))}
            </ul>
          </Card>
          {adding && (
            <ConnectForm
              scope={scope}
              onCancel={() => setAdding(false)}
              onDone={() => {
                setAdding(false);
                res.reload();
              }}
            />
          )}
        </>
      )}
    </Loaded>
  );
}

function ConnectForm({
  scope,
  onCancel,
  onDone,
}: {
  scope: MountScope;
  onCancel: () => void;
  onDone: () => void;
}) {
  const t = useT();
  const [provider, setProvider] = useState<MountProvider>('s3');
  const [label, setLabel] = useState('');
  const [endpoint, setEndpoint] = useState('');
  const [region, setRegion] = useState('');
  const [bucket, setBucket] = useState('');
  const [prefix, setPrefix] = useState('');
  const [accessKey, setAccessKey] = useState('');
  const [secret, setSecret] = useState('');
  const [pathStyle, setPathStyle] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const pick = (p: MountProvider) => {
    setProvider(p);
    setRegion(PROVIDER_DEFAULTS[p].region);
    setPathStyle(PROVIDER_DEFAULTS[p].pathStyle);
  };
  const ready = label.trim() && bucket.trim() && accessKey.trim() && secret.trim();
  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await createMount(scope, {
        label: label.trim(),
        provider,
        endpoint: endpoint.trim() || undefined,
        region: region.trim() || undefined,
        bucket: bucket.trim(),
        prefix: prefix.trim() || undefined,
        path_style: pathStyle,
        access_key_id: accessKey.trim(),
        secret_access_key: secret,
      });
      setSecret('');
      onDone();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card
      id="sto-connect"
      title={t('settings.storage.connectTitle')}
      description={t('settings.storage.connectDescription')}
    >
      <form
        className="sto-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (ready) void submit();
        }}
      >
        <div className="sto-field">
          <label id="sto-provider-l">{t('settings.storage.provider')}</label>
          <Select
            aria-labelledby="sto-provider-l"
            value={provider}
            onValueChange={(v) => pick(v as MountProvider)}
            options={MOUNT_PROVIDERS.map((p) => ({
              value: p,
              label: t(`settings.storage.providerName.${p}`),
            }))}
          />
        </div>
        <Notice variant="info" title={t('settings.storage.guidanceTitle')}>
          {t(`settings.storage.guidance.${provider}`)}{' '}
          {policyExample(provider, '', '') === null && t('settings.storage.tokenScopesNote')}
        </Notice>
        {policyExample(provider, bucket, prefix) !== null && (
          <PolicyBlock text={policyExample(provider, bucket, prefix) ?? ''} />
        )}
        <TextInput
          label={t('settings.storage.label')}
          value={label}
          onChange={(e) => setLabel(e.target.value)}
          maxLength={64}
        />
        <TextInput
          label={t('settings.storage.endpoint')}
          hint={t('settings.storage.endpointHint')}
          value={endpoint}
          onChange={(e) => setEndpoint(e.target.value)}
          inputMode="url"
        />
        <TextInput
          label={t('settings.storage.region')}
          value={region}
          onChange={(e) => setRegion(e.target.value)}
        />
        <TextInput
          label={t('settings.storage.bucket')}
          value={bucket}
          onChange={(e) => setBucket(e.target.value)}
        />
        <TextInput
          label={t('settings.storage.prefix')}
          hint={t('settings.storage.prefixHint')}
          value={prefix}
          onChange={(e) => setPrefix(e.target.value)}
        />
        <label className="sto-check">
          <input
            type="checkbox"
            checked={pathStyle}
            onChange={(e) => setPathStyle(e.target.checked)}
          />
          {t('settings.storage.pathStyle')}
        </label>
        <TextInput
          label={t('settings.storage.accessKey')}
          value={accessKey}
          onChange={(e) => setAccessKey(e.target.value)}
          autoComplete="off"
          spellCheck={false}
        />
        <TextInput
          label={t('settings.storage.secretKey')}
          hint={t('settings.storage.secretHint')}
          type="password"
          value={secret}
          onChange={(e) => setSecret(e.target.value)}
          autoComplete="off"
        />
        {error && (
          <Notice variant="danger" title={t('settings.storage.connectFailed')}>
            {error}
          </Notice>
        )}
        <div className="us-actions">
          <Button type="submit" loading={busy} disabled={!ready}>
            {t('settings.storage.save')}
          </Button>
          <Button type="button" variant="secondary" onClick={onCancel}>
            {t('settings.storage.cancel')}
          </Button>
        </div>
      </form>
    </Card>
  );
}

/** A copyable policy document with the "check against your provider's current docs" reminder. */
function PolicyBlock({ text }: { text: string }) {
  const t = useT();
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  };
  return (
    <div className="sto-policy">
      <p>
        <strong>{t('settings.storage.policyTitle')}</strong>
      </p>
      <pre tabIndex={0} aria-label={t('settings.storage.policyAria')}>
        <code>{text}</code>
      </pre>
      <div className="us-actions">
        <Button type="button" variant="secondary" size="sm" onClick={() => void copy()}>
          {copied ? t('settings.storage.policyCopied') : t('settings.storage.policyCopy')}
        </Button>
      </div>
      <p className="us-muted">{t('settings.storage.policyCheckDocs')}</p>
    </div>
  );
}

function MountRow({
  scope,
  mount,
  onChanged,
}: {
  scope: MountScope;
  mount: Mount;
  onChanged: () => void;
}) {
  const t = useT();
  const [test, setTest] = useState<TestResult | null>(mount.last_test);
  const [cors, setCors] = useState<CorsResult | null>(null);
  const [busy, setBusy] = useState<'test' | 'check' | 'apply' | 'toggle' | 'delete' | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const run = async (what: NonNullable<typeof busy>, fn: () => Promise<void>) => {
    setBusy(what);
    setError(null);
    try {
      await fn();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  };
  const corsReady = mount.cors_ok_at !== null || cors?.state === 'ok';

  return (
    <div className="sto-mount" data-status={mount.status}>
      <div className="sto-mount-head">
        <div>
          <strong>{mount.label}</strong>{' '}
          <span className="sto-badge" data-status={mount.status}>
            {t(`settings.storage.status.${mount.status}`)}
          </span>
          <p className="us-muted">
            {t(`settings.storage.providerName.${mount.provider}`)} / {mount.bucket}
            {mount.prefix ? `/${mount.prefix}` : ''} /{' '}
            {t('settings.storage.keyLine', { id: mount.access_key_id, hint: mount.key_hint })}
          </p>
        </div>
        <div className="us-actions">
          <Button
            variant="secondary"
            size="sm"
            loading={busy === 'test'}
            onClick={() =>
              void run('test', async () => {
                setTest(await testMount(scope, mount.id));
                onChanged();
              })
            }
          >
            {t('settings.storage.test')}
          </Button>
          <Link
            to="/storage/$mountId"
            params={{ mountId: mount.id }}
            search={{ prefix: '' }}
            className="sto-link"
          >
            {t('settings.storage.browse')}
          </Link>
          <Button
            variant="secondary"
            size="sm"
            loading={busy === 'toggle'}
            onClick={() =>
              void run('toggle', async () => {
                await patchMount(scope, mount.id, {
                  status: mount.status === 'disabled' ? 'active' : 'disabled',
                });
                onChanged();
              })
            }
          >
            {mount.status === 'disabled'
              ? t('settings.storage.enable')
              : t('settings.storage.disable')}
          </Button>
          {confirmDelete ? (
            <Button
              variant="danger"
              size="sm"
              loading={busy === 'delete'}
              onClick={() =>
                void run('delete', async () => {
                  await deleteMount(scope, mount.id);
                  onChanged();
                })
              }
            >
              {t('settings.storage.confirmRemove')}
            </Button>
          ) : (
            <Button variant="danger" size="sm" onClick={() => setConfirmDelete(true)}>
              {t('settings.storage.remove')}
            </Button>
          )}
        </div>
      </div>
      {error && (
        <Notice variant="danger" title={t('settings.storage.actionFailed')}>
          {error}
        </Notice>
      )}
      {test && (
        <table className="sto-table" aria-label={t('settings.storage.testTableAria')}>
          <caption className="us-muted">
            {t('settings.storage.testedAt', { at: fmtDate(test.at) })}
          </caption>
          <tbody>
            {test.rows.map((r) => (
              <tr key={r.id} data-status={r.status}>
                <th scope="row">{t(`settings.storage.probeStatus.${r.status}`)}</th>
                <td>{r.message}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <div className="sto-cors">
        <p>
          <strong>{t('settings.storage.corsTitle')}</strong>{' '}
          <span className="sto-badge" data-status={corsReady ? 'active' : 'failing'}>
            {corsReady ? t('settings.storage.corsReady') : t('settings.storage.corsNotReady')}
          </span>
        </p>
        <p className="us-muted">{t('settings.storage.corsDescription')}</p>
        {cors && cors.state !== 'ok' && (
          <>
            <p>{t(`settings.storage.corsState.${cors.state}`)}</p>
            {cors.state === 'manual' ? (
              cors.manual_steps && <p>{cors.manual_steps}</p>
            ) : (
              <ul className="sto-rules">
                {cors.required.map((r) => (
                  <li key={r}>{r}</li>
                ))}
              </ul>
            )}
          </>
        )}
        {cors?.state === 'ok' && <p role="status">{t('settings.storage.corsState.ok')}</p>}
        <div className="us-actions">
          <Button
            variant="secondary"
            size="sm"
            loading={busy === 'check'}
            onClick={() =>
              void run('check', async () => {
                setCors(await checkCors(scope, mount.id));
              })
            }
          >
            {t('settings.storage.corsCheck')}
          </Button>
          <Button
            size="sm"
            loading={busy === 'apply'}
            onClick={() =>
              void run('apply', async () => {
                setCors(await applyCors(scope, mount.id));
                onChanged();
              })
            }
          >
            {t('settings.storage.corsApply')}
          </Button>
        </div>
      </div>
    </div>
  );
}

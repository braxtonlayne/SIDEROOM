import { Button, Notice, TextInput, useToast } from '@sideroom/ui';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useState } from 'react';
import { useT } from '../../i18n';
import { createInteractionsKey, errorText, keys, verifyEndpoint } from '../api';
import { usePatchApp } from '../hooks';
import type { DevApp } from '../types';
import { CopyButton } from '../ui';

/** Same rules as the server: https, a host, no credentials, no fragment, at most 512 characters. */
export function endpointUrlError(raw: string): 'https' | 'invalid' | null {
  const v = raw.trim();
  if (!v) return null;
  if (!v.startsWith('https://')) return 'https';
  if (v.length > 512 || /[\s\\#]/.test(v)) return 'invalid';
  const authority = v.slice('https://'.length).split(/[/?]/)[0] ?? '';
  if (!authority || authority.startsWith(':') || authority.includes('@')) return 'invalid';
  return null;
}

/**
 * Interactions endpoint URL and public key (DEV-BOT-005). With a URL, the platform sends each interaction as a
 * signed POST to it instead of over the gateway. Saving first checks the URL with a signed PING and a bad-signature
 * probe, so the server's answer to a refused URL is shown as it comes.
 */
export function InteractionsEndpointCard({ app }: { app: DevApp }) {
  const t = useT();
  const toast = useToast();
  const qc = useQueryClient();
  const patch = usePatchApp(app.id);
  const current = app.interactions_endpoint_url ?? '';
  const [url, setUrl] = useState(current);
  const [error, setError] = useState<string | null>(null);
  const urlError = endpointUrlError(url);
  const dirty = url.trim() !== current;

  const makeKey = useMutation({
    mutationFn: () => createInteractionsKey(app.id),
    onSuccess: ({ verify_key }) => {
      qc.setQueryData<DevApp>(keys.app(app.id), (cur) => (cur ? { ...cur, verify_key } : cur));
    },
    onError: (e) => setError(errorText(e, t('developers.endpoint.keyFailed'))),
  });

  const reenable = useMutation({
    mutationFn: () => verifyEndpoint(app.id),
    onSuccess: ({ interactions_endpoint_state }) => {
      setError(null);
      qc.setQueryData<DevApp>(keys.app(app.id), (cur) =>
        cur ? { ...cur, interactions_endpoint_state } : cur,
      );
      toast.show({ title: t('developers.endpoint.reenabled'), variant: 'success' });
    },
    onError: (e) => setError(errorText(e, t('developers.endpoint.reenableFailed'))),
  });

  const save = (next: string | null) => {
    setError(null);
    patch.mutate(
      { interactions_endpoint_url: next },
      {
        onSuccess: () => {
          if (next === null) setUrl('');
          toast.show({
            title:
              next === null ? t('developers.endpoint.removed') : t('developers.endpoint.saved'),
            variant: 'success',
          });
        },
        onError: (e) => setError(errorText(e, t('developers.endpoint.saveFailed'))),
      },
    );
  };

  return (
    <section className="dev-card" aria-labelledby="dev-endpoint-h">
      <h2 id="dev-endpoint-h" className="dev-card-title">
        {t('developers.endpoint.heading')}
      </h2>
      <p className="dev-muted">{t('developers.endpoint.intro')}</p>
      {current && app.interactions_endpoint_state === 'disabled' && (
        <Notice variant="warning" title={t('developers.endpoint.disabledTitle')}>
          {t('developers.endpoint.disabledBody')}{' '}
          <Button
            variant="link"
            size="sm"
            loading={reenable.isPending}
            onClick={() => reenable.mutate()}
          >
            {t('developers.endpoint.reenable')}
          </Button>
        </Notice>
      )}
      {current && app.interactions_endpoint_state === 'enabled' && (
        <p className="dev-muted">{t('developers.endpoint.active')}</p>
      )}
      <div className="dev-field">
        <TextInput
          label={t('developers.endpoint.publicKey')}
          hint={t('developers.endpoint.publicKeyHint')}
          value={app.verify_key ?? ''}
          placeholder={t('developers.endpoint.noKey')}
          readOnly
        />
        <div className="dev-actions">
          {app.verify_key ? (
            <CopyButton text={app.verify_key} label={t('developers.endpoint.copyKey')} />
          ) : (
            <Button
              variant="secondary"
              loading={makeKey.isPending}
              onClick={() => makeKey.mutate()}
            >
              {t('developers.endpoint.createKey')}
            </Button>
          )}
        </div>
      </div>
      <TextInput
        label={t('developers.endpoint.urlLabel')}
        hint={t('developers.endpoint.urlHint')}
        placeholder={t('developers.endpoint.urlPlaceholder')}
        value={url}
        inputMode="url"
        onChange={(e) => setUrl(e.target.value)}
        error={
          urlError === 'https'
            ? t('developers.endpoint.errHttps')
            : urlError === 'invalid'
              ? t('developers.endpoint.errInvalid')
              : undefined
        }
      />
      {error && (
        <p className="dev-error" role="alert">
          {error}
        </p>
      )}
      <div className="dev-actions">
        <Button
          disabled={!dirty || !!urlError || !url.trim() || !app.verify_key}
          loading={patch.isPending}
          onClick={() => save(url.trim())}
        >
          {t('developers.endpoint.save')}
        </Button>
        {current && (
          <Button variant="ghost" disabled={patch.isPending} onClick={() => save(null)}>
            {t('developers.endpoint.remove')}
          </Button>
        )}
      </div>
    </section>
  );
}

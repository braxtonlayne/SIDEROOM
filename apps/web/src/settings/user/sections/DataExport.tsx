import { Button, Notice } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT, type MessageKey } from '../../../i18n';
import { errorText, getDataExport, requestDataExport, useResource, type DataExport } from '../api';
import { Card, fmtDate, Loaded } from '../parts';

const POLL_MS = 5000;

const STATE_KEYS: Record<string, string> = {
  none: 'settings.dataExport.stateNone',
  pending: 'settings.dataExport.statePending',
  building: 'settings.dataExport.stateBuilding',
  ready: 'settings.dataExport.stateReady',
  failed: 'settings.dataExport.stateFailed',
  expired: 'settings.dataExport.stateExpired',
};

function sizeLabel(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

/** Request my data (LNG-USET-019): status of the newest package and its download link. */
export function DataExportCard() {
  const t = useT();
  const res = useResource(getDataExport);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const state = res.data?.state;
  const working = state === 'pending' || state === 'building';
  const { setData } = res;
  useEffect(() => {
    if (!working) return;
    const id = window.setInterval(() => {
      getDataExport().then(
        (d) => setData(d),
        () => undefined,
      );
    }, POLL_MS);
    return () => window.clearInterval(id);
  }, [working, setData]);
  const request = () => {
    setError(null);
    setBusy(true);
    requestDataExport()
      .then(
        (d) => setData(d),
        (e: unknown) => setError(errorText(e)),
      )
      .finally(() => setBusy(false));
  };
  const link = (d: DataExport) =>
    d.download_url ? `/api/v1${d.download_url.replace(/^\/api\/v1/, '')}` : null;
  return (
    <Card
      id="us-data"
      title={t('settings.privacy.dataTitle')}
      description={t('settings.privacy.dataDesc')}
    >
      <Loaded res={res} what={t('settings.dataExport.what')}>
        {(d) => (
          <div className="us-stack">
            <p role="status">{t((STATE_KEYS[d.state] ?? STATE_KEYS.none) as MessageKey)}</p>
            {d.requested_at && (
              <p className="us-muted">
                {t('settings.dataExport.requestedAt', { when: fmtDate(d.requested_at) })}
              </p>
            )}
            {d.state === 'ready' && link(d) && (
              <p>
                <a className="us-link" href={link(d) ?? undefined} download>
                  {t('settings.dataExport.download')}
                </a>
                {typeof d.size_bytes === 'number' && (
                  <span className="us-muted"> ({sizeLabel(d.size_bytes)})</span>
                )}
                {d.expires_at && (
                  <span className="us-muted">
                    {' '}
                    {t('settings.dataExport.expiresAt', { when: fmtDate(d.expires_at) })}
                  </span>
                )}
              </p>
            )}
            <div className="us-actions">
              <Button variant="secondary" disabled={busy || working} onClick={request}>
                {d.state === 'none'
                  ? t('settings.privacy.dataTitle')
                  : t('settings.dataExport.requestAgain')}
              </Button>
            </div>
          </div>
        )}
      </Loaded>
      {error && (
        <Notice variant="danger" title={t('settings.dataExport.error')}>
          {error}
        </Notice>
      )}
    </Card>
  );
}

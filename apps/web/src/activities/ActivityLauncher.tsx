// The Activities shelf (DEV-ACT-004): the approved Activities a member can start in a voice channel.
import { Button, Modal, Notice } from '@sideroom/ui';
import { useState } from 'react';
import { ApiError } from '../api/client';
import { useT } from '../i18n';
import { httpActivityApi, type ActivityApi } from './api';
import { useShelf } from './hooks';
import type { ActivityInstance } from './types';

export function ActivityLauncher({
  open,
  onClose,
  channelId,
  onStarted,
  api = httpActivityApi,
}: {
  open: boolean;
  onClose: () => void;
  channelId: string;
  onStarted?: (i: ActivityInstance) => void;
  api?: ActivityApi;
}) {
  const t = useT();
  const { shelf, failed } = useShelf(open, api);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const start = async (applicationId: string) => {
    setBusy(applicationId);
    setError(null);
    try {
      const inst = await api.launch(channelId, applicationId);
      onStarted?.(inst);
      onClose();
    } catch (e) {
      if (e instanceof ApiError && e.status === 403) setError(t('activities.launcher.forbidden'));
      else if (e instanceof ApiError && e.status === 409)
        setError(t('activities.launcher.running'));
      else setError(t('activities.launcher.failed'));
    } finally {
      setBusy(null);
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('activities.launcher.title')}
      description={t('activities.launcher.description')}
      footer={
        <Button variant="ghost" onClick={onClose}>
          {t('activities.launcher.close')}
        </Button>
      }
    >
      <div className="act-shelf">
        {failed && (
          <Notice variant="danger" role="alert">
            {t('activities.launcher.loadFailed')}
          </Notice>
        )}
        {shelf && shelf.items.length === 0 && (
          <p className="act-muted">{t('activities.launcher.empty')}</p>
        )}
        <ul className="act-list" aria-label={t('activities.launcher.listLabel')}>
          {shelf?.items.map((a) => (
            <li key={a.application_id} className="act-item">
              <span className="act-item-text">
                <strong>{a.name}</strong>
                {a.development && (
                  <span className="act-badge">{t('activities.launcher.development')}</span>
                )}
                {a.description ? <span className="act-muted"> {a.description}</span> : null}
              </span>
              <Button
                size="sm"
                loading={busy === a.application_id}
                disabled={busy !== null}
                onClick={() => void start(a.application_id)}
                aria-label={t('activities.launcher.startLabel', { name: a.name })}
              >
                {t('activities.launcher.start')}
              </Button>
            </li>
          ))}
        </ul>
        {error && (
          <Notice variant="danger" role="alert">
            {error}
          </Notice>
        )}
      </div>
    </Modal>
  );
}

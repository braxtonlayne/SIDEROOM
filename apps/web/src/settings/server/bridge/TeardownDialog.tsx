import { Button, Modal, Notice } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { errorText } from '../api';
import * as api from './api';

/**
 * The owner's "Switch off" (BRG-UI-001, D-381): says what is removed, what stays and the Discord-side step, runs the
 * switch-off (the teardown happens on the server), then names any webhook Discord would not delete so the owner can
 * remove it by hand. `onClose` gets the module as it is afterwards (null when nothing changed).
 */
export function TeardownDialog({
  guildId,
  onClose,
}: {
  guildId: string;
  onClose: (after: api.BridgeFeature | null) => void;
}) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [after, setAfter] = useState<api.BridgeFeature | null>(null);

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      setAfter(await api.switchBridge(guildId, false));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  if (after) {
    const failed = after.teardown?.failed_webhooks ?? [];
    return (
      <Modal
        open
        onClose={() => onClose(after)}
        size="sm"
        title={t('settings.bridge.teardown.doneTitle')}
        footer={
          <Button onClick={() => onClose(after)}>{t('settings.bridge.teardown.close')}</Button>
        }
      >
        <p>{t('settings.bridge.teardown.doneBody', { n: after.teardown?.maps_removed ?? 0 })}</p>
        {failed.length > 0 ? (
          <Notice variant="warning" title={t('settings.bridge.teardown.failedTitle')}>
            <p>{t('settings.bridge.teardown.failedBody', { where: api.INTEGRATIONS_HINT })}</p>
            <ul aria-label={t('settings.bridge.teardown.failedList')}>
              {failed.map((id) => (
                <li key={id}>
                  <code>{id}</code>
                </li>
              ))}
            </ul>
          </Notice>
        ) : (
          <p>{t('settings.bridge.teardown.nothingLeft')}</p>
        )}
      </Modal>
    );
  }

  return (
    <Modal
      open
      onClose={() => onClose(null)}
      size="sm"
      title={t('settings.bridge.teardown.title')}
      footer={
        <>
          <Button variant="secondary" disabled={busy} onClick={() => onClose(null)}>
            {t('settings.bridge.teardown.cancel')}
          </Button>
          <Button variant="danger" disabled={busy} onClick={() => void run()}>
            {t('settings.bridge.teardown.confirm')}
          </Button>
        </>
      }
    >
      <h3 className="ss-bridge-h">{t('settings.bridge.teardown.removedTitle')}</h3>
      <ul>
        <li>{t('settings.bridge.teardown.removedWebhooks')}</li>
        <li>{t('settings.bridge.teardown.removedMaps')}</li>
        <li>{t('settings.bridge.teardown.removedToken')}</li>
      </ul>
      <h3 className="ss-bridge-h">{t('settings.bridge.teardown.staysTitle')}</h3>
      <ul>
        <li>{t('settings.bridge.teardown.staysMessages')}</li>
      </ul>
      <p>{t('settings.bridge.teardown.discordStep', { where: api.INTEGRATIONS_HINT })}</p>
      <p>{t('settings.bridge.teardown.again')}</p>
      {error && <Notice variant="danger">{error}</Notice>}
    </Modal>
  );
}

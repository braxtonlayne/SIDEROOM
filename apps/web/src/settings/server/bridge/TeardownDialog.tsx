import { Button, Checkbox, Modal, Notice } from '@sideroom/ui';
import { useEffect, useState } from 'react';
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
  whole = false,
  onClose,
}: {
  guildId: string;
  /** Whole-server mode (35b): offers what to do with Discord's side. */
  whole?: boolean;
  onClose: (after: api.BridgeFeature | null) => void;
}) {
  const t = useT();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [after, setAfter] = useState<api.BridgeFeature | null>(null);
  const [preview, setPreview] = useState<api.TeardownPreview | null>(null);
  const [removeCreated, setRemoveCreated] = useState(false);
  const [restore, setRestore] = useState(false);
  useEffect(() => {
    if (!whole) return;
    api
      .getTeardownPreview(guildId)
      .then(setPreview)
      .catch((e) => setError(errorText(e)));
  }, [whole, guildId]);

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      setAfter(
        whole
          ? await api.switchBridgeOff(guildId, {
              remove_created: removeCreated,
              restore_permissions: restore && !!preview?.snapshot_available,
            })
          : await api.switchBridge(guildId, false),
      );
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  if (after) {
    const failed = after.teardown?.failed_webhooks ?? [];
    const residue = after.teardown?.residue ?? [];
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
        {residue.length > 0 && (
          <Notice variant="warning" title={t('settings.bridgeSync.teardown.residueTitle')}>
            <p>{t('settings.bridgeSync.teardown.residueBody')}</p>
            <ul aria-label={t('settings.bridgeSync.teardown.residueList')}>
              {residue.map((id) => (
                <li key={id}>
                  <code>{id}</code>
                </li>
              ))}
            </ul>
          </Notice>
        )}
      </Modal>
    );
  }

  return (
    <Modal
      open
      onClose={() => onClose(null)}
      size="sm"
      title={
        whole ? t('settings.bridgeSync.teardown.title') : t('settings.bridge.teardown.title')
      }
      footer={
        <>
          <Button variant="secondary" disabled={busy} onClick={() => onClose(null)}>
            {t('settings.bridge.teardown.cancel')}
          </Button>
          <Button variant="danger" disabled={busy || (whole && !preview && !error)} onClick={() => void run()}>
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
      {whole && (
        <>
          <h3 className="ss-bridge-h">{t('settings.bridgeSync.teardown.leaveTitle')}</h3>
          <p>{t('settings.bridgeSync.teardown.leave')}</p>
          {!preview && !error && <p role="status">{t('settings.bridgeSync.teardown.loadingPreview')}</p>}
          {preview && (
            <>
              <Checkbox
                label={t('settings.bridgeSync.teardown.remove', {
                  roles: preview.created.roles,
                  categories: preview.created.categories,
                  channels: preview.created.channels,
                  threads: preview.created.threads,
                })}
                checked={removeCreated}
                disabled={busy}
                onChange={(e) => setRemoveCreated(e.target.checked)}
              />
              {removeCreated && preview.channels_with_discord_messages > 0 && (
                <Notice variant="warning">
                  {t('settings.bridgeSync.teardown.removeWarn', {
                    n: preview.channels_with_discord_messages,
                  })}
                </Notice>
              )}
              <Checkbox
                label={t('settings.bridgeSync.teardown.restore')}
                checked={restore && preview.snapshot_available}
                disabled={busy || !preview.snapshot_available}
                onChange={(e) => setRestore(e.target.checked)}
              />
              {!preview.snapshot_available && (
                <p className="ss-sub">{t('settings.bridgeSync.teardown.noSnapshot')}</p>
              )}
            </>
          )}
          <p>{t('settings.bridgeSync.teardown.neverDeletes')}</p>
          <p>{t('settings.bridgeSync.teardown.herestays')}</p>
        </>
      )}
      <p>{t('settings.bridge.teardown.discordStep', { where: api.INTEGRATIONS_HINT })}</p>
      <p>{t('settings.bridge.teardown.again')}</p>
      {error && <Notice variant="danger">{error}</Notice>}
    </Modal>
  );
}

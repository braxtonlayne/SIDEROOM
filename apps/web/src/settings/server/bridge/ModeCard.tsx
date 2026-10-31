import { Button, Modal, Notice } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../../i18n';
import { ActionStatus, useAction } from '../ui';
import * as api from './api';

/**
 * Mode card (BRG-SYN-004, design 15): "Only bridge these" or "Bridge the whole server". Only the server owner changes
 * the mode (with the MFA step); a manager can prepare and read a plan, so the owner has it ready. The mode is never a
 * permission switch: whichever is chosen, permissions are set on this server and Discord can only shrink what it
 * shows, never raise what anyone can do here.
 */
export function ModeCard({
  guildId,
  isOwner,
  status,
  onPrepare,
  onChanged,
}: {
  guildId: string;
  isOwner: boolean;
  status: api.BridgeStatus;
  /** Open the preflight and plan. */
  onPrepare: () => void;
  /** The mode changed; the screen reads the status again. */
  onChanged: () => Promise<void>;
}) {
  const t = useT();
  const act = useAction();
  const [confirmBack, setConfirmBack] = useState(false);
  const whole = status.mode === 'whole_guild';
  const connected = status.state === 'connected';

  const back = () =>
    void act
      .run(async () => {
        await api.setMode(guildId, { mode: 'selective' });
        await onChanged();
        return true;
      }, t('settings.bridgeSync.mode.backDone'))
      .then((ok) => {
        if (ok) setConfirmBack(false);
      });

  return (
    <section className="ss-card" aria-labelledby="ss-bridge-mode" data-testid="bridge-mode-card">
      <h2 id="ss-bridge-mode">{t('settings.bridgeSync.mode.title')}</h2>
      <p role="status" className="ss-bridge-mode-now">
        <strong>
          {t('settings.bridgeSync.mode.current', {
            mode: whole
              ? t('settings.bridgeSync.mode.wholeGuild')
              : t('settings.bridgeSync.mode.selective'),
          })}
        </strong>
      </p>
      <p className="ss-sub">
        {whole
          ? t('settings.bridgeSync.mode.wholeBody')
          : t('settings.bridgeSync.mode.selectiveBody')}
      </p>
      {!whole && (
        <>
          <Notice variant="warning" title={t('settings.bridgeSync.mode.adminWarningTitle')}>
            {t('settings.bridgeSync.mode.adminWarning')}
          </Notice>
          {!connected && <p className="ss-sub">{t('settings.bridgeSync.mode.needBot')}</p>}
          <div className="ss-actions">
            <Button variant="secondary" disabled={!connected} onClick={onPrepare}>
              {t('settings.bridgeSync.mode.prepare')}
            </Button>
          </div>
          <p className="ss-sub">{t('settings.bridgeSync.mode.prepareHint')}</p>
        </>
      )}
      {whole && (
        <div className="ss-actions">
          <Button
            variant="secondary"
            disabled={!isOwner || act.busy}
            onClick={() => setConfirmBack(true)}
          >
            {t('settings.bridgeSync.mode.back')}
          </Button>
        </div>
      )}
      {!isOwner && (
        <p className="ss-sub" data-testid="bridge-mode-owner-only">
          {t('settings.bridgeSync.mode.ownerOnly')}
        </p>
      )}
      <ActionStatus error={act.error} done={act.done} />
      {confirmBack && (
        <Modal
          open
          size="sm"
          onClose={() => setConfirmBack(false)}
          title={t('settings.bridgeSync.mode.backTitle')}
          footer={
            <>
              <Button variant="secondary" disabled={act.busy} onClick={() => setConfirmBack(false)}>
                {t('settings.bridgeSync.mode.cancel')}
              </Button>
              <Button disabled={act.busy} onClick={back}>
                {t('settings.bridgeSync.mode.backConfirm')}
              </Button>
            </>
          }
        >
          <p>{t('settings.bridgeSync.mode.backBody')}</p>
          {act.error && <Notice variant="danger">{act.error}</Notice>}
        </Modal>
      )}
    </section>
  );
}

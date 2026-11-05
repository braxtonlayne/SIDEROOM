// The Activities shelf (DEV-ACT-004): the approved Activities a member can start in a voice channel.
// With an Activity already running in the channel (`running`, or learnt from a 409 when two people start at once)
// the shelf gives way to a Join for that one: a channel runs one Activity at a time (XC-VIS-003 G5).
import { Button, Modal, Notice } from '@sideroom/ui';
import { useEffect, useRef, useState } from 'react';
import { ApiError } from '../api/client';
import { useT } from '../i18n';
import { httpActivityApi, type ActivityApi } from './api';
import { useShelf } from './hooks';
import { GameServersTile } from '../settings/server/gameservers/GameServersTile';
import { useGameServersOn } from '../settings/server/gameservers/flag';
import type { ActivityInstance } from './types';

export function ActivityLauncher({
  open,
  onClose,
  channelId,
  onStarted,
  running = null,
  applicationId,
  api = httpActivityApi,
}: {
  open: boolean;
  onClose: () => void;
  channelId: string;
  onStarted?: (i: ActivityInstance) => void;
  /** The channel's running instance, if the opener knows it: offered to join in place of the shelf. */
  running?: ActivityInstance | null;
  /**
   * A game chosen before the launcher opened (the Arcade's Play, D-472): launched at once, since picking the channel
   * was the confirmation. A refusal falls back to the shelf with the reason, as for any start.
   */
  applicationId?: string;
  api?: ActivityApi;
}) {
  const t = useT();
  const { shelf, failed } = useShelf(open, api);
  const gameServersOn = useGameServersOn();
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [current, setCurrent] = useState<ActivityInstance | null>(running);

  const start = async (applicationId: string) => {
    setBusy(applicationId);
    setError(null);
    try {
      const inst = await api.launch(channelId, applicationId);
      onStarted?.(inst);
      onClose();
    } catch (e) {
      if (e instanceof ApiError && e.status === 403) setError(t('activities.launcher.forbidden'));
      else if (e instanceof ApiError && e.status === 409) {
        // Someone started one first: offer that one when the channel says which, else say so.
        const now = await api.inChannel(channelId).catch(() => null);
        if (now?.instance) setCurrent(now.instance);
        else setError(t('activities.launcher.running'));
      } else setError(t('activities.launcher.failed'));
    } finally {
      setBusy(null);
    }
  };

  const join = async (inst: ActivityInstance) => {
    setBusy(inst.id);
    setError(null);
    try {
      const joined = await api.join(inst.id);
      onStarted?.(joined);
      onClose();
    } catch {
      setError(t('activities.launcher.failed'));
    } finally {
      setBusy(null);
    }
  };

  // Launch the preselected game once per opening, unless the channel already runs one (then offer to join it).
  const autoStarted = useRef(false);
  useEffect(() => {
    if (!open) {
      autoStarted.current = false;
      return;
    }
    if (!applicationId || current || autoStarted.current) return;
    autoStarted.current = true;
    void start(applicationId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, applicationId, current]);

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
        {current ? (
          <div className="act-item">
            <span className="act-item-text">
              {t('activities.launcher.runningNamed', { name: current.application_name })}
            </span>
            <Button
              size="sm"
              loading={busy === current.id}
              disabled={busy !== null}
              onClick={() => void join(current)}
              aria-label={t('activities.button.join', { name: current.application_name })}
            >
              {t('activities.launcher.join')}
            </Button>
          </div>
        ) : (
          <>
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
            {gameServersOn && <GameServersTile />}
          </>
        )}
        {error && (
          <Notice variant="danger" role="alert">
            {error}
          </Notice>
        )}
      </div>
    </Modal>
  );
}

import { useEffect } from 'react';
import { Button, Modal, Notice } from '@sideroom/ui';
import { useT } from '../../i18n';
import { useCurrentUser, useUsers } from '../../lounge/data';
import { useVoiceSession } from '../../voice/session';
import { coopSession } from './session';
import { useCoopSource } from './source';
import { useCoop } from './useCoop';

/**
 * "Play together" (ACT-EMU-002). The host picks the running game and shares the screen with the existing
 * screen share; the others pick the host and get a seat. Opening the dialog starts looking for hosts in the
 * call, closing it stops (a seated guest keeps playing; the chip in the voice panel is their handle).
 */
export function CoopDialog({
  open,
  onClose,
  onShare,
}: {
  open: boolean;
  onClose: () => void;
  /** Open the screen share picker (the voice panel owns it). */
  onShare: () => void;
}) {
  const t = useT();
  const me = useCurrentUser();
  const users = useUsers();
  const pingMs = useVoiceSession((s) => s.quality.pingMs);
  const game = useCoopSource((s) => s.game);
  const coop = useCoop();
  const name = (id: string) => users[id]?.displayName ?? users[id]?.username ?? id;

  useEffect(() => {
    if (!open) return;
    coopSession.watch(me.id);
    return () => coopSession.unwatch();
  }, [open, me.id]);

  const hosting = coop.host?.running ? coop.host : null;
  const g = coop.guest;

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('emu.coop.dialog.title')}
      description={t('emu.coop.dialog.description')}
      size="md"
      footer={<Button onClick={onClose}>{t('emu.coop.dialog.close')}</Button>}
    >
      <div className="vc-picker" data-testid="coop-dialog">
        <Notice variant="info" data-testid="coop-no-rollback">
          {t('emu.coop.noRollback')}
        </Notice>

        <section aria-labelledby="coop-host-h">
          <h3 id="coop-host-h">{t('emu.coop.host.heading')}</h3>
          {hosting ? (
            <>
              <p>{t('emu.coop.host.hosting', { title: hosting.title })}</p>
              <p>{t('emu.coop.host.shareHint')}</p>
              {pingMs !== null && <p>{t('emu.coop.host.ping', { ms: pingMs })}</p>}
              {hosting.seats.length === 0 ? (
                <p>{t('emu.coop.host.noPlayers')}</p>
              ) : (
                <ul>
                  {hosting.seats.map((s) => (
                    <li key={s.userId}>
                      {t('emu.coop.host.seat', { n: s.port + 1, name: name(s.userId) })}{' '}
                      <Button
                        size="sm"
                        variant="secondary"
                        aria-label={t('emu.coop.host.kick', { name: name(s.userId) })}
                        onClick={() => coopSession.kick(s.userId)}
                      >
                        {t('emu.coop.host.kick', { name: name(s.userId) })}
                      </Button>
                    </li>
                  ))}
                </ul>
              )}
              <div className="vc-error-actions">
                <Button size="sm" variant="secondary" onClick={onShare}>
                  {t('emu.coop.host.share')}
                </Button>
                <Button size="sm" variant="secondary" onClick={() => coopSession.stop()}>
                  {t('emu.coop.host.stop')}
                </Button>
              </div>
            </>
          ) : game ? (
            <Button
              onClick={() =>
                coopSession.startHosting({
                  game,
                  selfId: me.id,
                  getPingMs: () => useVoiceSession.getState().quality.pingMs,
                })
              }
            >
              {t('emu.coop.host.start', { title: game.title })}
            </Button>
          ) : (
            <p data-testid="coop-no-game">{t('emu.coop.host.noGame')}</p>
          )}
        </section>

        {!hosting && (
          <section aria-labelledby="coop-guest-h">
            <h3 id="coop-guest-h">{t('emu.coop.guest.heading')}</h3>
            {g && g.hosts.length > 0 ? (
              <ul>
                {g.hosts.map((h) => (
                  <li key={h.userId} data-testid="coop-host-row">
                    {t('emu.coop.guest.hostLine', {
                      name: name(h.userId),
                      title: h.title,
                      free: h.free,
                    })}{' '}
                    <Button
                      size="sm"
                      disabled={h.free === 0 || g.phase === 'seated' || g.phase === 'joining'}
                      onClick={() => coopSession.join(h.userId)}
                    >
                      {t('emu.coop.guest.join')}
                    </Button>
                  </li>
                ))}
              </ul>
            ) : (
              <p>{t('emu.coop.guest.none')}</p>
            )}
            {g?.phase === 'joining' && <p role="status">{t('emu.coop.guest.joining')}</p>}
            {g?.phase === 'full' && <p role="status">{t('emu.coop.guest.full')}</p>}
            {g?.phase === 'closed' && <p role="status">{t('emu.coop.guest.closed')}</p>}
            {g?.phase === 'kicked' && <p role="status">{t('emu.coop.guest.kicked')}</p>}
            <p>{t('emu.coop.guest.keys')}</p>
          </section>
        )}
      </div>
    </Modal>
  );
}

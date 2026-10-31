import { Button, Notice } from '@sideroom/ui';
import { useT } from '../../i18n';
import { coopSession } from './session';
import { useCoop } from './useCoop';
import './coop.css';

/**
 * The strip in the voice panel while a co-op game runs: who you are, the measured input round trip (the
 * "video RTT" of ACT-EMU-002, with the reminder that the picture adds its own delay), the controls switch and
 * the way out. Renders nothing otherwise.
 */
export function CoopStatus() {
  const t = useT();
  const coop = useCoop();
  const host = coop.host?.running ? coop.host : null;
  const guest = coop.guest?.phase === 'seated' ? coop.guest : null;
  if (!host && !guest) return null;

  return (
    <Notice
      variant="info"
      role="status"
      data-testid="coop-status"
      aria-label={t('emu.coop.status.region')}
    >
      {host ? (
        <span data-testid="coop-status-text">
          {t('emu.coop.status.hosting', { title: host.title, count: host.seats.length })}
        </span>
      ) : guest ? (
        <>
          <span data-testid="coop-status-text">
            {t('emu.coop.status.player', {
              n: (guest.port ?? 0) + 1,
              title: guest.hosts.find((h) => h.userId === guest.hostId)?.title ?? '',
            })}
          </span>{' '}
          <span data-testid="coop-latency">
            {guest.rttMs === null
              ? t('emu.coop.status.measuring')
              : t('emu.coop.status.latency', { ms: guest.rttMs })}
          </span>
        </>
      ) : null}
      <div className="vc-error-actions coop-status-actions">
        {guest && (
          <Button
            size="sm"
            variant="secondary"
            aria-pressed={coop.controls}
            onClick={() => coopSession.setControls(!coop.controls)}
          >
            {coop.controls ? t('emu.coop.status.controlsOn') : t('emu.coop.status.controlsOff')}
          </Button>
        )}
        <Button size="sm" variant="secondary" onClick={() => coopSession.stop()}>
          {host ? t('emu.coop.status.stop') : t('emu.coop.status.leave')}
        </Button>
      </div>
    </Notice>
  );
}

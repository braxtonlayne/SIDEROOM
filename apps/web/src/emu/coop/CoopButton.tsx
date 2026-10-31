import { Icon, Tooltip } from '@sideroom/ui';
import { lazy, Suspense, useEffect, useState } from 'react';
import { useT } from '../../i18n';
import { isInVoice, useVoiceSession } from '../../voice/session';
import { coopSession } from './session';
import './source'; // registers the e2e offer hook
import { useCoop } from './useCoop';

const Dialog = lazy(() => import('./CoopDialog').then((m) => ({ default: m.CoopDialog })));

/**
 * The "Play together" entry of the voice panel (ACT-EMU-002), and the status chip while a co-op game runs.
 * The dialog loads on first use; this file is small and holds no emulator code.
 */
export function CoopButton({ onShare }: { onShare: () => void }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const inVoice = useVoiceSession((s) => isInVoice(s));
  const coop = useCoop();

  // Leaving the call ends the session (the data channel goes with it).
  useEffect(() => {
    if (!inVoice) coopSession.stop();
  }, [inVoice]);

  const active = Boolean(coop.host?.running) || coop.guest?.phase === 'seated';
  return (
    <>
      <Tooltip content={t('emu.coop.button')}>
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={t('emu.coop.button')}
          data-active={active || undefined}
          data-testid="coop-button"
          onClick={() => setOpen(true)}
        >
          <Icon name="play" size={20} />
        </button>
      </Tooltip>
      {open && (
        <Suspense fallback={null}>
          <Dialog open onClose={() => setOpen(false)} onShare={onShare} />
        </Suspense>
      )}
    </>
  );
}

import { Notice } from '@sideroom/ui';
import { useT } from '../i18n';
import { useClientFlags } from '../moderation/reports';

/** The emulator kill switch (`platform.emulator`, OPS-FLG-001). Only an explicit `false` counts. */
export function useEmulatorOn(): boolean {
  return useClientFlags((s) => s.flags['platform.emulator'] !== false);
}

/** Shown on `/games` while the operator has switched the emulator off. */
export function EmulatorSwitchedOff() {
  const t = useT();
  return (
    <main className="emu-off">
      <Notice variant="info" title={t('emu.switchedOff.title')}>
        {t('emu.switchedOff.body')}
      </Notice>
    </main>
  );
}

// The slowmode presets panel of the Features settings (lounge.slowmode_plus); a feature slot (featureSlots.tsx).
import { useT } from '../../../i18n';
import { SlowmodeForm } from '../../../lounge/features/SlowmodeForm';
import type { FeatureSlotProps } from './featureSlots';

export default function SlowmodePanel({ guildId }: FeatureSlotProps) {
  const t = useT();
  return (
    <section aria-label={t('lounge.slowmode.panelTitle')}>
      <h4 className="ss-sub">{t('lounge.slowmode.panelTitle')}</h4>
      <p className="ss-meta">{t('lounge.slowmode.panelHint')}</p>
      <SlowmodeForm guildId={guildId} />
    </section>
  );
}

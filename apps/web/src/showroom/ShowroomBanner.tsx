import { useT } from '../i18n';
import { setShowroom, useShowroom, useShowroomActive } from './state';
import './showroom.css';

/** The banner every preview screen shows, so a tester never mistakes sample data for real data. */
export function ShowroomBanner() {
  const t = useT();
  const active = useShowroomActive();
  const hint = useShowroom((s) => s.hint);
  if (!active) return null;
  return (
    <div className="srm-banner" data-testid="showroom-banner">
      <strong>{t('showroom.banner.title')}</strong>
      <span>{t('showroom.banner.body')}</span>
      <span className="srm-hint" role="status" aria-live="polite" data-testid="showroom-hint">
        {hint ?? ''}
      </span>
      <button type="button" className="srm-banner-off" onClick={() => setShowroom(false)}>
        {t('showroom.banner.off')}
      </button>
    </div>
  );
}

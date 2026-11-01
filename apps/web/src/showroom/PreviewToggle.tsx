import { useT } from '../i18n';
import { setShowroom, useShowroom, useShowroomAvailable } from './state';
import './showroom.css';

/** The top-bar toggle. Rendered only where the deployment switch `platform.showroom` is on. */
export function PreviewToggle() {
  const t = useT();
  const available = useShowroomAvailable();
  const on = useShowroom((s) => s.on);
  if (!available) return null;
  return (
    <button
      type="button"
      className="srm-toggle"
      aria-pressed={on}
      data-testid="showroom-toggle"
      onClick={() => setShowroom(!on)}
    >
      {t('showroom.toggle.label')}
    </button>
  );
}

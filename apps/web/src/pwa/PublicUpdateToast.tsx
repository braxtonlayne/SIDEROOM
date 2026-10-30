import { useState } from 'react';
import { useT } from '../i18n/react';
import './pwa.css';
import { applyPendingUpdate, usePwaState } from './state';

/**
 * The update toast for the public surfaces (SEC-ENT-002). The same `pwa.update.*` strings and card as the app's
 * `PwaHost`, and nothing else: no install prompt, no notices bar, no player deferral. Plain buttons keep it
 * inside every surface's small closure. Mount it once in each public shell beside the router.
 */
export function PublicUpdateToast() {
  const t = useT();
  const pwa = usePwaState();
  const [hidden, setHidden] = useState(false);
  if (!pwa.updateReady || hidden) return null;
  return (
    <div className="sr-pwa" data-testid="pwa-host">
      <div className="sr-pwa-card" role="status" aria-live="polite">
        <div>
          <strong>{t('pwa.update.title')}</strong>
          <p>{t('pwa.update.body')}</p>
        </div>
        <div className="sr-pwa-actions">
          <button type="button" className="sr-pwa-primary" onClick={applyPendingUpdate}>
            {t('pwa.update.action')}
          </button>
          <button type="button" className="sr-pwa-secondary" onClick={() => setHidden(true)}>
            {t('pwa.update.dismiss')}
          </button>
        </div>
      </div>
    </div>
  );
}

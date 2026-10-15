import { OutlineButton, GoldButton } from '@sideroom/ui';
import { useEffect, useState } from 'react';
import { useT } from '../i18n/react';
import './pwa.css';
import { applyPendingUpdate, isIosSafari, promptInstall, usePwaState } from './state';

const DISMISS_KEY = 'sr.pwa.install.dismissed';
const DISMISS_MS = 30 * 24 * 60 * 60 * 1000;
const SHOW_AFTER_MS = 20_000;
/** Pages where an install nudge would interrupt a first impression. */
const QUIET =
  /^\/($|login|register|forgot|reset|join|verify-email|no-invitation|house-notes|invite)/;

function dismissedRecently(): boolean {
  try {
    const at = Number(localStorage.getItem(DISMISS_KEY));
    return at > 0 && Date.now() - at < DISMISS_MS;
  } catch {
    return false;
  }
}

/**
 * Update toast (XC-PWA-002) and install prompt UX (XC-PWA-003): a Chromium-style prompt on desktop and
 * Android, and "Add to Home Screen" steps on iOS Safari. Both are polite `status` regions, never modal.
 */
export function PwaHost({ now = 20_000 }: { now?: number } = {}) {
  const t = useT();
  const pwa = usePwaState();
  const [ready, setReady] = useState(false);
  const [hidden, setHidden] = useState(dismissedRecently);
  const [updateHidden, setUpdateHidden] = useState(false);

  useEffect(() => {
    const id = setTimeout(() => setReady(true), Math.min(now, SHOW_AFTER_MS));
    return () => clearTimeout(id);
  }, [now]);

  const dismissInstall = () => {
    setHidden(true);
    try {
      localStorage.setItem(DISMISS_KEY, String(Date.now()));
    } catch {
      /* ignore */
    }
  };

  const ios = isIosSafari();
  const showInstall =
    ready && !hidden && !pwa.installed && (pwa.canInstall || ios) && !QUIET.test(location.pathname);

  return (
    <div className="sr-pwa" data-testid="pwa-host">
      {pwa.updateReady && !updateHidden && !pwa.noticesBar && (
        <div className="sr-pwa-card" role="status" aria-live="polite">
          <div>
            <strong>{t('pwa.update.title')}</strong>
            <p>{t('pwa.update.body')}</p>
          </div>
          <div className="sr-pwa-actions">
            <GoldButton type="button" onClick={applyPendingUpdate}>
              {t('pwa.update.action')}
            </GoldButton>
            <OutlineButton type="button" onClick={() => setUpdateHidden(true)}>
              {t('pwa.update.dismiss')}
            </OutlineButton>
          </div>
        </div>
      )}
      {showInstall && (
        <div className="sr-pwa-card" role="status" aria-live="polite">
          {pwa.canInstall ? (
            <>
              <div>
                <strong>{t('pwa.install.title')}</strong>
                <p>{t('pwa.install.body')}</p>
              </div>
              <div className="sr-pwa-actions">
                <GoldButton
                  type="button"
                  onClick={() => {
                    void promptInstall().then(() => dismissInstall());
                  }}
                >
                  {t('pwa.install.action')}
                </GoldButton>
                <OutlineButton type="button" onClick={dismissInstall}>
                  {t('pwa.install.dismiss')}
                </OutlineButton>
              </div>
            </>
          ) : (
            <>
              <div>
                <strong>{t('pwa.install.ios.title')}</strong>
                <ol>
                  <li>{t('pwa.install.ios.step1')}</li>
                  <li>{t('pwa.install.ios.step2')}</li>
                  <li>{t('pwa.install.ios.step3')}</li>
                </ol>
              </div>
              <div className="sr-pwa-actions">
                <OutlineButton type="button" onClick={dismissInstall}>
                  {t('pwa.install.dismiss')}
                </OutlineButton>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}

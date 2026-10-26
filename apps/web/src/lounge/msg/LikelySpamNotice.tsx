import { useState, type ReactNode } from 'react';
import { useT } from '../../i18n';

/**
 * PAR-FU-002 (U-03): a friend's message that the recipient's "Filter all" DM spam level flagged stays in the
 * conversation behind a "likely spam" notice (Discord's shape). The recipient can reveal it and hide it again; the
 * choice lasts until the message leaves the screen. The author never sees the notice on their own message.
 */
export function LikelySpamGate({ flagged, children }: { flagged: boolean; children: ReactNode }) {
  const t = useT();
  const [shown, setShown] = useState(false);
  if (!flagged) return <>{children}</>;
  return (
    <>
      <p className="lng-likely-spam" role="note" data-testid="likely-spam-notice">
        <span>{t('lounge.likelySpam.notice')}</span>{' '}
        <button
          type="button"
          className="lng-link-btn"
          aria-expanded={shown}
          onClick={() => setShown((v) => !v)}
        >
          {shown ? t('lounge.likelySpam.hide') : t('lounge.likelySpam.show')}
        </button>
      </p>
      {shown && children}
    </>
  );
}

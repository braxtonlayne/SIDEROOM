// An empty watch party room (D-227, H-02): nothing is playing until its host picks something. The host (or anyone
// who may control playback) searches the Cinema and starts it; everybody else sees that the host is choosing. The
// server runs the play gate, the party limit and the age rule when the media is set.
import { Button } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../i18n';
import { httpPartyApi } from './api';
import { partyController, useParty } from './runtime';
import { TitlePicker, type PickedTitle } from './TitlePicker';

export function PickMedia() {
  const t = useT();
  const c = partyController();
  const party = useParty((s) => s.party);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (!party || party.media) return null;
  const canPick = !!party.you?.can_control;
  const pick = async (p: PickedTitle) => {
    setError(null);
    setBusy(true);
    try {
      c.adoptRest(
        await httpPartyApi.changeMedia(party.id, { title_id: p.titleId, episode_id: p.episodeId }),
      );
    } catch {
      setError(t('watchparty.empty.pickFailed'));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="wp-pick" role="region" aria-label={t('watchparty.empty.region')}>
      {canPick ? (
        <>
          <h2 className="wp-h">{t('watchparty.empty.pickHeading')}</h2>
          <TitlePicker onPick={(p) => void pick(p)} actionLabel={t('watchparty.empty.watch')} />
          {busy && <p className="wp-muted">{t('watchparty.empty.starting')}</p>}
          {error && (
            <p role="alert" className="wp-error">
              {error}
              <Button size="sm" variant="ghost" onClick={() => setError(null)}>
                {t('watchparty.empty.dismiss')}
              </Button>
            </p>
          )}
        </>
      ) : (
        <p role="status">{t('watchparty.empty.waiting')}</p>
      )}
    </div>
  );
}

// The Screening Room of a voice channel (STU-SHF-001, wave 41-B): Watch Together and the Live TV guide live here, as
// a social activity next to the call, not as a storefront. Picking what to watch starts from the shelves the group
// can see (The Study); the existing party strip shows a running party.
import { Button, Icon } from '@sideroom/ui';
import { useRouter } from '@tanstack/react-router';
import { lazy, Suspense, useState } from 'react';
import { useT } from '../../i18n';
import { useChannelParty } from '../../watchparty/useChannelParty';
import { WatchPartyStrip } from '../../watchparty/WatchPartyStrip';
import './voice-p41-screening.css';

const Dialog = lazy(() =>
  import('../../watchparty/StartPartyDialog').then((m) => ({ default: m.StartPartyDialog })),
);

export function ScreeningRoom({ channelId }: { channelId: string }) {
  const t = useT();
  const router = useRouter();
  const party = useChannelParty(channelId);
  const [open, setOpen] = useState(false);
  return (
    <details className="vc-screening" open={party ? true : undefined}>
      <summary>
        <Icon name="film" size={16} /> {t('lounge.screeningRoom.title')}
      </summary>
      <div className="vc-screening-body">
        <p className="vc-screening-hint">{t('lounge.screeningRoom.hint')}</p>
        <div className="vc-screening-actions">
          <Button
            size="sm"
            onClick={() => (party ? router.history.push(`/cinema/party/${party.id}`) : setOpen(true))}
          >
            {party ? t('lounge.screeningRoom.join') : t('lounge.screeningRoom.start')}
          </Button>
          <Button size="sm" variant="secondary" onClick={() => router.history.push('/study')}>
            {t('lounge.screeningRoom.shelves')}
          </Button>
          <Button size="sm" variant="secondary" onClick={() => router.history.push('/cinema/tv')}>
            {t('lounge.screeningRoom.guide')}
          </Button>
        </div>
        <WatchPartyStrip channelId={channelId} />
      </div>
      {open && (
        <Suspense fallback={null}>
          <Dialog open onClose={() => setOpen(false)} channelId={channelId} />
        </Suspense>
      )}
    </details>
  );
}

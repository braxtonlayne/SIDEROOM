// "Start Watch Party" in the voice call bar (spec section 2): the party binds to that voice channel and the call
// carries on as normal. When the channel already has a party it says "Join Watch Party" instead.
import { Icon } from '@sideroom/ui';
import { useRouter } from '@tanstack/react-router';
import { lazy, Suspense, useState } from 'react';
import { useChannelParty } from './useChannelParty';

const Dialog = lazy(() =>
  import('./StartPartyDialog').then((m) => ({ default: m.StartPartyDialog })),
);

export function StartPartyButton({ channelId }: { channelId: string }) {
  const party = useChannelParty(channelId);
  const router = useRouter();
  const [open, setOpen] = useState(false);
  const label = party ? 'Join Watch Party' : 'Start Watch Party';
  return (
    <>
      <button
        type="button"
        className="vc-ctl"
        aria-label={label}
        title={label}
        onClick={() => (party ? router.history.push(`/cinema/party/${party.id}`) : setOpen(true))}
      >
        <Icon name="film" size={24} />
      </button>
      {open && (
        <Suspense fallback={null}>
          <Dialog open onClose={() => setOpen(false)} channelId={channelId} />
        </Suspense>
      )}
    </>
  );
}

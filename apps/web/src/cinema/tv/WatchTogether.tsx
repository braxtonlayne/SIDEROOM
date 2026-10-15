// "Watch together" on a Live TV channel (G-16, WP-FE-008): starts a party on the channel. Everyone in it is on
// the same live edge, so there is nothing to seek; the party adds the chat, reactions and voice.
import { Icon } from '@sideroom/ui';
import { Suspense, lazy, useState } from 'react';
import { useT } from '../../i18n';
import type { TvChannel } from './types';

// Loaded on first use: the destination picker pulls in the Lounge data hooks.
const StartPartyDialog = lazy(() =>
  import('../../watchparty/StartPartyDialog').then((m) => ({ default: m.StartPartyDialog })),
);

export function WatchTogether({
  channel,
  className,
  iconOnly,
}: {
  channel: Pick<TvChannel, 'id' | 'number' | 'name'>;
  className?: string;
  iconOnly?: boolean;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const label = t('cinema.watchTogether.button');
  return (
    <>
      <button
        type="button"
        className={className}
        aria-label={iconOnly ? label : undefined}
        title={iconOnly ? label : undefined}
        onClick={() => setOpen(true)}
      >
        <Icon name="users" size={iconOnly ? 20 : 16} />
        {!iconOnly && <> {label}</>}
      </button>
      {open && (
        <Suspense fallback={null}>
          <StartPartyDialog
            open
            onClose={() => setOpen(false)}
            liveChannel={{
              channelId: channel.id,
              label: t('cinema.watchTogether.channelLabel', {
                number: channel.number,
                name: channel.name,
              }),
            }}
          />
        </Suspense>
      )}
    </>
  );
}

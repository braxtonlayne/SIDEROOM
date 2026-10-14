// The live watch party of a channel or DM, for the cards in the Lounge: one REST read when the channel opens,
// then kept fresh by the gateway (`WATCH_PARTY_UPDATE`). Plain state, no query cache (the Lounge has none).
import { useEffect, useState } from 'react';
import { onDispatch } from '../store/events';
import { httpPartyApi } from './api';
import type { Party, PartyUpdateEvent } from './types';

export function useChannelParty(channelId: string | undefined): Party | null {
  const [party, setParty] = useState<Party | null>(null);

  useEffect(() => {
    setParty(null);
    if (!channelId) return;
    let dead = false;
    httpPartyApi
      .inChannel(channelId)
      .then((p) => !dead && setParty(p.ended ? null : p))
      .catch(() => undefined); // 404: no party here
    const off = onDispatch((event, data) => {
      if (event !== 'WATCH_PARTY_UPDATE') return;
      const e = data as PartyUpdateEvent;
      if (!('party' in e) || e.party.channel_id !== channelId) return;
      setParty(e.party.ended ? null : e.party);
    });
    return () => {
      dead = true;
      off();
    };
  }, [channelId]);

  return party;
}

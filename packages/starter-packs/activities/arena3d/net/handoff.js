// @ts-check
// Host migration, the game half (D-388; `host_handoff` in sideroom.json). The platform moves the host when it
// leaves: the server names the longest-connected member, the SDK follows (`sdk.on('host')`) and net/sdk-channel.js
// turns that into `channel.onHostChange`. This is what each page then does with its role:
//
//   - the member that is the new host rebuilds a HostSession from the newest `host:snap` it heard and the seat table
//     (`host:seats`): same world, same slots, the old host's player gone, every other player kept;
//   - every other member says hello to the new host (PredictedClient.hostChanged), which gives it its slot back
//     and takes its unacknowledged inputs; prediction carries on meanwhile;
//   - a host that is told it no longer hosts steps down to a peer.
//
// Ticks never go backwards for the match as a whole: the new host resumes from a snapshot that is at most a few
// ticks (and one network trip) older than the last tick the old host ran, and every peer rewinds to it on its next
// snapshot (net/client.js onSnapshot). What the successor itself loses: the inputs it predicted since that
// snapshot, which is a few ticks of its own walking.

import { HostSession } from './host.js';
import { PredictedClient } from './client.js';

/**
 * @param {{ channel: import('./channel.js').Channel, world: import('../sim/world.js').World, name: string,
 *   host: HostSession | null, client: PredictedClient | null }} s
 * @returns {{ host: HostSession | null, client: PredictedClient | null, change: 'promoted' | 'demoted' | 'rejoined' | 'none' }}
 */
export function handoffRole(s) {
  const { channel, world, name } = s;
  if (channel.isHost && !s.host) {
    // Without a snapshot this is a fresh match on the same world (peers hello again and get new seats).
    const resume = s.client?.takeover() ?? { selfSlot: 0 };
    return { host: new HostSession({ channel, world, name, resume }), client: null, change: 'promoted' };
  }
  if (!channel.isHost && s.host) {
    s.host.close();
    return { host: null, client: new PredictedClient({ channel, world, name }), change: 'demoted' };
  }
  if (!channel.isHost && s.client) {
    s.client.hostChanged();
    return { host: null, client: s.client, change: 'rejoined' };
  }
  return { host: s.host, client: s.client, change: 'none' };
}

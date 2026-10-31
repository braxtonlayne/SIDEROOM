// The channels of this person's servers that the Discord bridge mirrors (BRG-UI-001, D-381): one request for the
// whole account (`GET /users/@me/bridge/channels`), asked once the Lounge shows a server channel and again when the
// answer is old. It answers an empty list while no bridge is on anywhere, so a server that never switched the
// module on costs this one small request and nothing else. Nothing here runs before sign-in (D-385).
import { useEffect } from 'react';
import { create } from 'zustand';
import {
  listBridgedChannels,
  setOptOut,
  type BridgeDirection,
  type BridgedChannel,
} from '../../settings/server/bridge/api';

/** How long an answer is trusted before the next channel view asks again. */
export const BRIDGE_STALE_MS = 5 * 60 * 1000;

interface BridgeState {
  channels: BridgedChannel[];
  loadedAt: number | null;
  inflight: boolean;
  load(force?: boolean): Promise<void>;
  /** Opt out (or back in) in one channel; rolls back when the server refuses. */
  optOut(channelId: string, out: boolean): Promise<void>;
  reset(): void;
}

export const useBridge = create<BridgeState>()((set, get) => ({
  channels: [],
  loadedAt: null,
  inflight: false,
  load: async (force = false) => {
    const { loadedAt, inflight } = get();
    if (inflight) return;
    if (!force && loadedAt !== null && Date.now() - loadedAt < BRIDGE_STALE_MS) return;
    set({ inflight: true });
    try {
      set({ channels: await listBridgedChannels(), loadedAt: Date.now(), inflight: false });
    } catch {
      // Unreachable or refused: no banner. Asked again on the next channel view after the stale window.
      set({ channels: [], loadedAt: Date.now(), inflight: false });
    }
  },
  optOut: async (channelId, out) => {
    const before = get().channels;
    set({
      channels: before.map((c) => (c.channel_id === channelId ? { ...c, opted_out: out } : c)),
    });
    try {
      await setOptOut(channelId, out);
    } catch (e) {
      set({ channels: before });
      throw e;
    }
  },
  reset: () => set({ channels: [], loadedAt: null, inflight: false }),
}));

/** The mapping of one channel, or `undefined` when it is not mirrored. Asks for the list on first use. */
export function useBridgedChannel(channelId: string | undefined): BridgedChannel | undefined {
  const load = useBridge((s) => s.load);
  useEffect(() => {
    void load();
  }, [load, channelId]);
  return useBridge((s) => s.channels.find((c) => c.channel_id === channelId));
}

/** Does this direction carry the member's own messages to Discord (so an opt-out means something)? */
export const mirrorsOutbound = (d: BridgeDirection) => d !== 'from_discord';

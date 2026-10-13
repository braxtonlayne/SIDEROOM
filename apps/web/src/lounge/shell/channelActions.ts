import { ApiError } from '../../api/client';
import { useStore } from '../../store';
import type { ChannelMove } from './channelMoves';
import { deleteChannelRequest, reorderChannelsRequest } from './api';

/**
 * Applies a reorder plan: the list updates at once, the server call follows, and the previous order comes
 * back when the server refuses (for example without Manage Channels).
 */
export async function applyChannelMoves(guildId: string, moves: ChannelMove[]): Promise<void> {
  const st = useStore.getState();
  const before = moves.map((m) => {
    const c = st.channels[m.id];
    return {
      id: m.id,
      guild_id: guildId,
      position: c?.position ?? 0,
      parent_id: c?.parent_id ?? null,
    };
  });
  for (const m of moves)
    st.applyDispatch('CHANNEL_UPDATE', {
      id: m.id,
      guild_id: guildId,
      position: m.position,
      parent_id: m.parentId,
    });
  try {
    await reorderChannelsRequest(
      guildId,
      moves.map((m) => ({ id: m.id, position: m.position, parent_id: m.parentId })),
    );
  } catch (e) {
    for (const b of before) useStore.getState().applyDispatch('CHANNEL_UPDATE', b);
    throw e;
  }
}

/** Deletes a channel or category and removes it from the list without waiting for the gateway. */
export async function deleteChannelAndForget(guildId: string, channelId: string): Promise<void> {
  await deleteChannelRequest(channelId);
  const st = useStore.getState();
  // Channels of a deleted category stay and move to the top level.
  for (const c of Object.values(st.channels))
    if (c.parent_id === channelId)
      st.applyDispatch('CHANNEL_UPDATE', { id: c.id, guild_id: guildId, parent_id: null });
  st.applyDispatch('CHANNEL_DELETE', { id: channelId, guild_id: guildId });
}

export const channelErrorText = (e: unknown, fallback: string) =>
  e instanceof ApiError && e.status === 403
    ? 'You do not have permission to do that here.'
    : e instanceof ApiError && e.message
      ? e.message
      : fallback;

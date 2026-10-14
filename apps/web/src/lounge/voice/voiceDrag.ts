/** Dragging people between voice channels (VOX-UI-009). The drop calls the moderator move route. */
import type { DragEvent } from 'react';
import { moderateVoiceState } from './actions';

export const VOICE_USER_MIME = 'application/x-sideroom-voice-user';

export interface VoiceDragData {
  userId: string;
  guildId: string;
  fromChannelId: string;
}

export function startVoiceDrag(e: DragEvent, data: VoiceDragData): void {
  e.dataTransfer.setData(VOICE_USER_MIME, JSON.stringify(data));
  e.dataTransfer.setData('text/plain', data.userId);
  e.dataTransfer.effectAllowed = 'move';
}

/** True while a person (not a file or a channel) is being dragged over the target. */
export const isVoiceDrag = (e: DragEvent): boolean =>
  Array.from(e.dataTransfer?.types ?? []).includes(VOICE_USER_MIME);

export function readVoiceDrag(e: DragEvent): VoiceDragData | null {
  try {
    const raw = e.dataTransfer?.getData(VOICE_USER_MIME);
    const d = raw ? (JSON.parse(raw) as Partial<VoiceDragData>) : null;
    return d && typeof d.userId === 'string' && typeof d.guildId === 'string'
      ? { userId: d.userId, guildId: d.guildId, fromChannelId: d.fromChannelId ?? '' }
      : null;
  } catch {
    return null;
  }
}

/**
 * Move a dragged person into a channel. Only inside the same server; the server re-checks MOVE_MEMBERS on
 * the target channel and answers with the new voice state (VOICE_STATE_UPDATE).
 */
export function dropVoiceUser(data: VoiceDragData, guildId: string, channelId: string) {
  if (data.guildId !== guildId || data.fromChannelId === channelId) return Promise.resolve();
  return moderateVoiceState(guildId, data.userId, { channel_id: channelId });
}

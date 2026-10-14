/** Voice actions the Lounge UI calls: joining from a channel row and the moderator REST routes. */
import { useCallback } from 'react';
import { apiFetch } from '../../api/client';
import { voiceController } from '../../voice/controller';
import { useLoungeGoto } from '../nav';
import type { Channel } from '../data';

export interface ModerateVoiceBody {
  /** A channel id moves the user, null disconnects them. */
  channel_id?: string | null;
  mute?: boolean;
  deaf?: boolean;
  reason?: string;
}

/** `PATCH /guilds/{guild}/voice-states/{user}` (docs/05 section 6.5). The server checks every permission. */
export const moderateVoiceState = (guildId: string, userId: string, body: ModerateVoiceBody) =>
  apiFetch<void>(`/guilds/${guildId}/voice-states/${userId}`, {
    method: 'PATCH',
    json: body,
    retries: 0,
  });

/** Join a voice channel and open its view (the Discord behaviour of clicking a voice channel). */
export function useJoinVoice() {
  const goto = useLoungeGoto();
  return useCallback(
    (channel: Pick<Channel, 'id' | 'guildId' | 'name' | 'permissions'>) => {
      voiceController().join({
        guildId: channel.guildId,
        channelId: channel.id,
        name: channel.name,
        pttForced: channel.permissions?.useVad === false,
      });
      goto(channel.guildId, channel.id);
    },
    [goto],
  );
}

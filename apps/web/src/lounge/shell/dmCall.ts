import { apiFetch } from '../../api/client';
import { DM_GUILD_ID, voiceController } from '../../voice/controller';
import { useVoiceSession } from '../../voice/session';
import { FREE_PROFILES } from '../../voice/video';

/**
 * Start a call in a DM or group DM (LNG-DM-004): join the call, ring the other people
 * (`POST /channels/{id}/call/ring`) and, for a video call, turn the camera on once connected.
 */
export function startDmCall(channelId: string, name: string, withVideo: boolean): void {
  const controller = voiceController();
  controller.join({ guildId: DM_GUILD_ID, channelId, name });
  void apiFetch<unknown>(`/channels/${encodeURIComponent(channelId)}/call/ring`, {
    method: 'POST',
    json: {},
    retries: 0,
  }).catch(() => undefined);
  if (!withVideo) return;
  const off = useVoiceSession.subscribe((s) => {
    if (s.channelId !== channelId) return off();
    if (s.status === 'connected') {
      off();
      void controller.startCamera(FREE_PROFILES);
    }
  });
}

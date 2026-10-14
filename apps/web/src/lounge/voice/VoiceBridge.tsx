import { useEffect } from 'react';
import { voiceController } from '../../voice/controller';
import { useVoiceSession } from '../../voice/session';
import { useVoicePerms } from './perms';
import { useVoiceAnnouncements } from '../../a11y/voiceAnnounce';

/**
 * Mounted once at the app root (not inside the Lounge shell), so a call survives navigating to User Settings,
 * the Library or the Cinema (VOX-CLI-010). It starts the key listeners and keeps "push-to-talk is forced"
 * in step with the connected channel's USE_VAD permission.
 */
export function VoiceBridge() {
  const guildId = useVoiceSession((s) => s.guildId);
  const channelId = useVoiceSession((s) => s.channelId);
  const perms = useVoicePerms(guildId ?? undefined, channelId ?? undefined);

  useVoiceAnnouncements();
  useEffect(() => voiceController().start(), []);

  useEffect(() => {
    if (channelId) voiceController().setPttForced(!perms.useVad);
  }, [channelId, perms.useVad]);

  return null;
}

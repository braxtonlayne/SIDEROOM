import { useEffect } from 'react';
import { onDispatch } from '../../store/events';
import { voiceController } from '../../voice/controller';
import { applyStatusEvent } from '../../voice/channelStatus';
import { applyStageEvent } from '../../voice/stage';
import { startVoiceTimerListener } from '../../tools/timers/voiceTimer';
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
  // Stage instances (VOX-STG-002) arrive with GUILD_CREATE and STAGE_INSTANCE_*; keep them whatever view is open.
  useEffect(() => onDispatch((event, data) => applyStageEvent(event, data)), []);
  // Voice channel statuses (VOX-UI-015) arrive with GUILD_CREATE and VOICE_CHANNEL_STATUS_UPDATE.
  useEffect(() => onDispatch((event, data) => applyStatusEvent(event, data)), []);
  // Timers shared with a voice channel (UTL-DEV-001) arrive as VOICE_TIMER_UPDATE.
  useEffect(() => startVoiceTimerListener(), []);

  useEffect(() => {
    if (channelId) voiceController().setPttForced(!perms.useVad);
  }, [channelId, perms.useVad]);

  return null;
}

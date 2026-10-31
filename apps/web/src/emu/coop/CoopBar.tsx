import { Icon, Tooltip } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { ScreenSharePicker } from '../../lounge/voice/ScreenSharePicker';
import { DM_GUILD_ID, voiceController } from '../../voice/controller';
import { useVideoLimits } from '../../voice/limits';
import { isInVoice, useVoiceSession } from '../../voice/session';
import { useLocalVideo } from '../../voice/video';
import { CoopButton } from './CoopButton';
import { CoopStatus } from './CoopStatus';
import '../../lounge/voice/voice-p10.css';

/**
 * The call strip of the Games page (ACT-EMU-001 + ACT-EMU-002 wiring). The game runs on /games, outside the lounge, so
 * the lounge's voice panel is not on screen while it plays; this strip carries the two things a host needs there:
 * "Play together" (host the running game) and the screen share of the game tab. Renders nothing outside a server's
 * voice call (co-op is offered in server calls only, like Activities).
 */
export function CoopBar() {
  const t = useT();
  const s = useVoiceSession();
  const limits = useVideoLimits(s.guildId);
  const screen = useLocalVideo((st) => st.screen);
  const [picker, setPicker] = useState(false);
  if (!isInVoice(s) || !s.guildId || !s.channelId || s.guildId === DM_GUILD_ID) return null;
  const controller = voiceController();

  return (
    <div
      className="vc-panel emu-coopbar"
      role="region"
      aria-label={t('lounge.voicePanel.region')}
      data-testid="emu-coopbar"
    >
      <div className="vc-panel-actions">
        <span className="emu-dim">{s.channelName}</span>
        <Tooltip
          content={s.screen ? t('lounge.voicePanel.stopShare') : t('lounge.voicePanel.share')}
        >
          <button
            type="button"
            className="lng-icon-btn"
            aria-label={s.screen ? t('lounge.voicePanel.stopShare') : t('lounge.voicePanel.share')}
            aria-pressed={s.screen}
            data-active={s.screen || undefined}
            onClick={() => (s.screen ? controller.stopScreenShare() : setPicker(true))}
          >
            <Icon name="screen-share" size={20} />
          </button>
        </Tooltip>
        <CoopButton onShare={() => setPicker(true)} />
      </div>
      <CoopStatus />
      {picker && (
        <ScreenSharePicker
          open
          onClose={() => setPicker(false)}
          profiles={limits.profiles}
          onStart={(opts) => void controller.startScreenShare(opts)}
          initial={
            screen
              ? {
                  source: screen.source,
                  audio: screen.audio,
                  hint: screen.hint,
                  presetId: screen.preset.id,
                }
              : undefined
          }
        />
      )}
    </div>
  );
}

// MUS-JB-001 (wave-31 integration): the listener's music controls. Music played into the call (the jukebox DJ or a
// bot) has its own volume and mute, apart from the voices; muting also tells the SFU to stop sending the track to
// this member. Kept on this device with the other voice settings.
import { Icon, Slider } from '@sideroom/ui';
import { useT } from '../../i18n';
import { voiceController } from '../controller';
import { useVoiceSettings } from '../settings';

export function MusicControls() {
  const t = useT();
  const volume = useVoiceSettings((s) => s.settings.musicVolume);
  const muted = useVoiceSettings((s) => s.settings.musicMuted);
  return (
    <div className="jb-music" role="group" aria-label={t('lounge.music.label')}>
      <button
        type="button"
        className="lng-icon-btn"
        aria-pressed={muted}
        aria-label={muted ? t('lounge.music.unmute') : t('lounge.music.mute')}
        title={t('lounge.music.hint')}
        data-testid="music-mute"
        onClick={() => voiceController().setMusicMuted(!muted)}
      >
        <Icon name={muted ? 'volume-off' : 'volume'} size={16} />
      </button>
      <Slider
        label={t('lounge.music.volume')}
        min={0}
        max={200}
        step={5}
        value={volume}
        disabled={muted}
        showValue
        formatValue={(v) => `${v}%`}
        onValueChange={(v) => voiceController().setMusicVolume(v)}
      />
    </div>
  );
}

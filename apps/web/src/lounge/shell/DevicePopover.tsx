import { Button, Icon, Popover, Slider } from '@sideroom/ui';
import { useId } from 'react';
import { useT } from '../../i18n';
import { useOpenUserSettings } from '../../settings/user/nav';
import { useAudioDevices } from '../../voice/hooks';
import { useVoiceSettings, VOLUME_RANGE } from '../../voice/settings';

const canPickOutput = () =>
  typeof HTMLMediaElement !== 'undefined' && 'setSinkId' in HTMLMediaElement.prototype;

/**
 * The arrow beside the mic or headphone button in the user panel (LNG-HOME-012/013): choose the device, set its
 * volume, jump to Voice settings. Choices are the same device-local voice settings the Voice & Video page edits.
 */
export function DevicePopover({ kind }: { kind: 'input' | 'output' }) {
  const t = useT();
  const name = useId();
  const input = kind === 'input';
  const { inputs, outputs } = useAudioDevices();
  const devices = (input ? inputs : outputs).filter((d) => d.deviceId && d.deviceId !== 'default');
  const deviceId = useVoiceSettings((s) =>
    input ? s.settings.inputDeviceId : s.settings.outputDeviceId,
  );
  const volume = useVoiceSettings((s) =>
    input ? s.settings.inputVolume : s.settings.outputVolume,
  );
  const update = useVoiceSettings((s) => s.update);
  const openSettings = useOpenUserSettings();
  const label = input ? t('lounge.devicePopover.micMenu') : t('lounge.devicePopover.headphoneMenu');
  const choose = (id: string) => update(input ? { inputDeviceId: id } : { outputDeviceId: id });
  const options = [
    { value: 'default', label: t('lounge.devicePopover.defaultDevice') },
    ...devices.map((d) => ({
      value: d.deviceId,
      label: d.label || t('lounge.devicePopover.unnamedDevice'),
    })),
  ];
  const unsupported = !input && !canPickOutput();

  return (
    <Popover
      label={label}
      placement="top"
      className="lng-device-pop"
      trigger={
        <button type="button" className="lng-icon-btn lng-device-arrow" aria-label={label}>
          <Icon name="chevron-up" size={16} />
        </button>
      }
    >
      <div className="lng-device">
        <fieldset className="lng-device-list">
          <legend>
            {input ? t('lounge.devicePopover.inputDevice') : t('lounge.devicePopover.outputDevice')}
          </legend>
          {options.map((o) => (
            <label key={o.value} className="lng-device-opt">
              <input
                type="radio"
                name={name}
                value={o.value}
                checked={deviceId === o.value}
                disabled={unsupported && o.value !== 'default'}
                onChange={() => choose(o.value)}
              />
              <span>{o.label}</span>
            </label>
          ))}
          {unsupported && <p className="lng-muted">{t('lounge.devicePopover.noOutputChoice')}</p>}
        </fieldset>
        <Slider
          label={
            input ? t('lounge.devicePopover.inputVolume') : t('lounge.devicePopover.outputVolume')
          }
          min={VOLUME_RANGE[0]}
          max={VOLUME_RANGE[1]}
          step={5}
          value={volume}
          showValue
          formatValue={(v) => `${v}%`}
          onValueChange={(v) => update(input ? { inputVolume: v } : { outputVolume: v })}
        />
        <Button size="sm" variant="secondary" onClick={() => openSettings('voice')}>
          {t('lounge.devicePopover.voiceSettings')}
        </Button>
      </div>
    </Popover>
  );
}

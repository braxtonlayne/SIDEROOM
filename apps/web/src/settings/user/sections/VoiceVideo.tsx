import { Button, Notice, SegmentedControl, Select, Slider, Switch } from '@sideroom/ui';
import { useMemo } from 'react';
import { RemoteAudio } from '../../../voice/playback';
import { useAudioDevices, useMicPermission, useMicTest } from '../../../voice/hooks';
import { CuePlayer, type Cue } from '../../../voice/sounds';
import { browserAudioDeps } from '../../../voice/deps';
import {
  keybindConflict,
  PTT_RELEASE_RANGE,
  useVoiceSettings,
  VOLUME_RANGE,
} from '../../../voice/settings';
import { levelFraction, THRESHOLD_RANGE } from '../../../voice/vad';
import { KeybindRecorder } from '../KeybindRecorder';
import { Card, SectionTitle } from '../parts';

const pct = (n: number) => `${n}%`;

/** A level bar for the microphone test; `thresholdDb` draws the voice-activity threshold on it. */
export function LevelMeter({
  db,
  label,
  thresholdDb,
  active,
}: {
  db: number;
  label: string;
  thresholdDb?: number;
  active: boolean;
}) {
  const level = active ? levelFraction(db) : 0;
  return (
    <div
      className="us-meter"
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(level * 100)}
      aria-valuetext={active ? `${Math.round(db)} dB` : 'Not testing'}
    >
      <div className="us-meter-fill" style={{ width: `${level * 100}%` }} />
      {thresholdDb !== undefined && (
        <div
          className="us-meter-mark"
          aria-hidden="true"
          style={{ left: `${levelFraction(thresholdDb) * 100}%` }}
        />
      )}
    </div>
  );
}

const PERMISSION_HELP: Record<string, string> = {
  denied:
    'Microphone access is blocked for this site. Allow it from the lock icon in your browser address bar, then press "Check again".',
};

export function VoiceVideoSection() {
  const settings = useVoiceSettings((s) => s.settings);
  const update = useVoiceSettings((s) => s.update);
  const setKeybind = useVoiceSettings((s) => s.setKeybind);
  const devices = useAudioDevices();
  const permission = useMicPermission();
  const test = useMicTest({
    deviceId: settings.inputDeviceId,
    echoCancellation: settings.echoCancellation,
    autoGainControl: settings.autoGainControl,
    inputVolume: settings.inputVolume,
  });
  const cues = useMemo(() => new CuePlayer(() => browserAudioDeps.createAudioContext()), []);
  const canPickOutput = RemoteAudio.canSelectOutput();

  const inputOptions = [
    { value: 'default', label: 'Default' },
    ...devices.inputs
      .filter((d) => d.deviceId && d.deviceId !== 'default')
      .map((d, i) => ({ value: d.deviceId, label: d.label || `Microphone ${i + 1}` })),
  ];
  const outputOptions = [
    { value: 'default', label: 'Default' },
    ...devices.outputs
      .filter((d) => d.deviceId && d.deviceId !== 'default')
      .map((d, i) => ({ value: d.deviceId, label: d.label || `Speakers ${i + 1}` })),
  ];
  const ptt = settings.inputMode === 'ptt';
  const grant = async () => {
    await test.start();
    await devices.refresh();
    permission.recheck();
  };
  const testCue = (cue: Cue) => cues.play(cue, settings.soundVolume / 100);

  return (
    <>
      <SectionTitle>Voice &amp; Video</SectionTitle>
      <p className="us-muted">These settings apply right away and stay on this device.</p>

      <Card id="us-voice-devices" title="Devices">
        {(permission.status === 'denied' || test.error?.code === 'permission-denied') && (
          <Notice variant="warning" title="Microphone blocked" className="us-gap">
            {PERMISSION_HELP.denied}{' '}
            <Button
              variant="link"
              size="sm"
              onClick={() => {
                permission.recheck();
                void grant();
              }}
            >
              Check again
            </Button>
          </Notice>
        )}
        {test.error && test.error.code !== 'permission-denied' && (
          <Notice variant="danger" title="Microphone problem" className="us-gap" role="alert">
            {test.error.message}
          </Notice>
        )}
        {!devices.labelled && permission.status !== 'denied' && (
          <Notice variant="info" className="us-gap">
            Allow microphone access to see your devices by name.{' '}
            <Button variant="link" size="sm" onClick={() => void grant()}>
              Allow microphone
            </Button>
          </Notice>
        )}
        <div className="us-form-grid">
          <div>
            <label id="us-in-dev" className="us-label">
              Input device
            </label>
            <Select
              aria-labelledby="us-in-dev"
              options={inputOptions}
              value={settings.inputDeviceId}
              onValueChange={(v) => update({ inputDeviceId: v })}
            />
          </div>
          <div>
            <label id="us-out-dev" className="us-label">
              Output device
            </label>
            <Select
              aria-labelledby="us-out-dev"
              options={outputOptions}
              value={settings.outputDeviceId}
              disabled={!canPickOutput}
              onValueChange={(v) => update({ outputDeviceId: v })}
            />
            {!canPickOutput && (
              <p className="us-muted">
                This browser can only play through your system&apos;s default output.
              </p>
            )}
          </div>
          <Slider
            label="Input volume"
            min={VOLUME_RANGE[0]}
            max={VOLUME_RANGE[1]}
            step={5}
            value={settings.inputVolume}
            showValue
            formatValue={pct}
            onValueChange={(v) => update({ inputVolume: v })}
          />
          <Slider
            label="Output volume"
            min={VOLUME_RANGE[0]}
            max={VOLUME_RANGE[1]}
            step={5}
            value={settings.outputVolume}
            showValue
            formatValue={pct}
            onValueChange={(v) => update({ outputVolume: v })}
          />
        </div>
        <div className="us-mic-test">
          <Button
            variant="secondary"
            onClick={() => (test.running ? test.stop() : void grant())}
            aria-pressed={test.running}
          >
            {test.running ? 'Stop testing' : "Let's check"}
          </Button>
          <LevelMeter db={test.level} active={test.running} label="Microphone level" />
        </div>
      </Card>

      <Card
        id="us-voice-mode"
        title="Input mode"
        description="Choose how your microphone opens. A channel can still require push-to-talk."
      >
        <SegmentedControl
          label="Input mode"
          value={settings.inputMode}
          onValueChange={(v) => update({ inputMode: v === 'ptt' ? 'ptt' : 'vad' })}
          options={[
            { value: 'vad', label: 'Voice Activity' },
            { value: 'ptt', label: 'Push to Talk' },
          ]}
        />
        {!ptt && (
          <div className="us-form-stack">
            <Switch
              label="Automatically determine input sensitivity"
              checked={settings.autoSensitivity}
              onChange={(e) => update({ autoSensitivity: e.target.checked })}
            />
            <Slider
              label="Input sensitivity"
              min={THRESHOLD_RANGE[0]}
              max={THRESHOLD_RANGE[1]}
              step={1}
              value={settings.thresholdDb}
              disabled={settings.autoSensitivity}
              showValue
              formatValue={(v) => `${v} dB`}
              onValueChange={(v) => update({ thresholdDb: v })}
            />
            <LevelMeter
              db={test.level}
              active={test.running}
              label="Input sensitivity meter"
              {...(settings.autoSensitivity ? {} : { thresholdDb: settings.thresholdDb })}
            />
            <p className="us-muted">
              {test.running
                ? 'Speak to see where your voice lands. The line marks where your microphone opens.'
                : 'Press "Let\'s check" above to see your microphone level here.'}
            </p>
          </div>
        )}
        {ptt && (
          <div className="us-form-stack">
            <div className="us-row-field">
              <span id="us-ptt-key-l" className="us-label">
                Shortcut
              </span>
              <KeybindRecorder
                action="Push to Talk"
                value={settings.keybinds.ptt}
                onChange={(b) => setKeybind('ptt', b)}
                conflict={(b) => keybindConflict(settings.keybinds, 'ptt', b)}
              />
            </div>
            {!settings.keybinds.ptt && (
              <Notice variant="warning" className="us-gap">
                Pick a push-to-talk key, otherwise nobody will hear you.
              </Notice>
            )}
            <Slider
              label="Push to talk release delay"
              min={PTT_RELEASE_RANGE[0]}
              max={PTT_RELEASE_RANGE[1]}
              step={50}
              value={settings.pttReleaseMs}
              showValue
              formatValue={(v) => `${v} ms`}
              onValueChange={(v) => update({ pttReleaseMs: v })}
            />
            <p className="us-muted">
              Keybinds work while this tab is focused. A system-wide push-to-talk needs a native
              app.
            </p>
          </div>
        )}
      </Card>

      <Card id="us-voice-processing" title="Audio processing">
        <Switch
          label="Echo cancellation"
          description="Stops your speakers from feeding back into your microphone."
          checked={settings.echoCancellation}
          onChange={(e) => update({ echoCancellation: e.target.checked })}
        />
        <Switch
          label="Automatic gain control"
          description="Evens out how loud you are."
          checked={settings.autoGainControl}
          onChange={(e) => update({ autoGainControl: e.target.checked })}
        />
        <p className="us-muted">
          There is no noise suppression: your voice is sent as your microphone hears it.
        </p>
      </Card>

      <Card id="us-voice-sounds" title="Sounds">
        <Switch
          label="Join and leave sounds"
          checked={settings.soundJoinLeave}
          onChange={(e) => update({ soundJoinLeave: e.target.checked })}
        />
        <Switch
          label="Mute and deafen sounds"
          checked={settings.soundMuteDeafen}
          onChange={(e) => update({ soundMuteDeafen: e.target.checked })}
        />
        <Switch
          label="Push to talk sounds"
          checked={settings.soundPtt}
          onChange={(e) => update({ soundPtt: e.target.checked })}
        />
        <Slider
          label="Sound volume"
          min={0}
          max={100}
          step={5}
          value={settings.soundVolume}
          showValue
          formatValue={pct}
          onValueChange={(v) => update({ soundVolume: v })}
        />
        <Button variant="secondary" size="sm" onClick={() => testCue('selfJoin')}>
          Play a sample
        </Button>
      </Card>

      <Notice variant="info" className="us-gap">
        Camera, screen sharing defaults and the soundboard arrive with the video update.
      </Notice>
    </>
  );
}

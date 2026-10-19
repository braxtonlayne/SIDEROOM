import { Button, Notice, SegmentedControl, Select, Slider, Switch } from '@sideroom/ui';
import { useMemo, useState } from 'react';
import { useT } from '../../../i18n';
import { RemoteAudio } from '../../../voice/playback';
import { useAudioDevices, useCameras, useMicPermission, useMicTest } from '../../../voice/hooks';
import { CameraPreview } from '../../../lounge/voice/CameraPreview';
import { SHARE_PRESETS, presetLabel } from '../../../voice/video';
import { useSoundboard } from '../../../voice/soundboard';
import { CuePlayer, type Cue } from '../../../voice/sounds';
import { browserAudioDeps } from '../../../voice/deps';
import {
  keybindConflict,
  PTT_RELEASE_RANGE,
  useVoiceSettings,
  VOLUME_RANGE,
} from '../../../voice/settings';
import { levelFraction, THRESHOLD_RANGE } from '../../../voice/vad';
import { usePrefsStore } from '../prefs';
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
  const t = useT();
  const level = active ? levelFraction(db) : 0;
  return (
    <div
      className="us-meter"
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(level * 100)}
      aria-valuetext={active ? `${Math.round(db)} dB` : t('settings.voice.notTesting')}
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

export function VoiceVideoSection() {
  const t = useT();
  const settings = useVoiceSettings((s) => s.settings);
  const update = useVoiceSettings((s) => s.update);
  const prefs = usePrefsStore((s) => s.prefs);
  const savePrefs = usePrefsStore((s) => s.save);
  const setKeybind = useVoiceSettings((s) => s.setKeybind);
  const soundboardVolume = useSoundboard((s) => s.volume);
  const setSoundboardVolume = useSoundboard((s) => s.setVolume);
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
  const { cameras, refresh: refreshCameras } = useCameras();
  const [previewOn, setPreviewOn] = useState(false);
  const cameraOptions = [
    { value: 'default', label: t('settings.voice.default') },
    ...cameras
      .filter((d) => d.deviceId && d.deviceId !== 'default')
      .map((d, i) => ({
        value: d.deviceId,
        label: d.label || t('settings.voice.cameraN', { n: i + 1 }),
      })),
  ];

  const inputOptions = [
    { value: 'default', label: t('settings.voice.default') },
    ...devices.inputs
      .filter((d) => d.deviceId && d.deviceId !== 'default')
      .map((d, i) => ({
        value: d.deviceId,
        label: d.label || t('settings.voice.microphoneN', { n: i + 1 }),
      })),
  ];
  const outputOptions = [
    { value: 'default', label: t('settings.voice.default') },
    ...devices.outputs
      .filter((d) => d.deviceId && d.deviceId !== 'default')
      .map((d, i) => ({
        value: d.deviceId,
        label: d.label || t('settings.voice.speakersN', { n: i + 1 }),
      })),
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
      <SectionTitle>{t('settings.voice.title')}</SectionTitle>
      <p className="us-muted">{t('settings.voice.intro')}</p>

      <Card id="us-voice-devices" title={t('settings.voice.devices')}>
        {(permission.status === 'denied' || test.error?.code === 'permission-denied') && (
          <Notice variant="warning" title={t('settings.voice.blockedTitle')} className="us-gap">
            {t('settings.voice.blockedHelp')}{' '}
            <Button
              variant="link"
              size="sm"
              onClick={() => {
                permission.recheck();
                void grant();
              }}
            >
              {t('settings.voice.checkAgain')}
            </Button>
          </Notice>
        )}
        {test.error && test.error.code !== 'permission-denied' && (
          <Notice
            variant="danger"
            title={t('settings.voice.problemTitle')}
            className="us-gap"
            role="alert"
          >
            {test.error.message}
          </Notice>
        )}
        {!devices.labelled && permission.status !== 'denied' && (
          <Notice variant="info" className="us-gap">
            {t('settings.voice.allowHint')}{' '}
            <Button variant="link" size="sm" onClick={() => void grant()}>
              {t('settings.voice.allowMic')}
            </Button>
          </Notice>
        )}
        <div className="us-form-grid">
          <div>
            <label id="us-in-dev" className="us-label">
              {t('settings.voice.inputDevice')}
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
              {t('settings.voice.outputDevice')}
            </label>
            <Select
              aria-labelledby="us-out-dev"
              options={outputOptions}
              value={settings.outputDeviceId}
              disabled={!canPickOutput}
              onValueChange={(v) => update({ outputDeviceId: v })}
            />
            {!canPickOutput && <p className="us-muted">{t('settings.voice.defaultOutputOnly')}</p>}
          </div>
          <Slider
            label={t('settings.voice.inputVolume')}
            min={VOLUME_RANGE[0]}
            max={VOLUME_RANGE[1]}
            step={5}
            value={settings.inputVolume}
            showValue
            formatValue={pct}
            onValueChange={(v) => update({ inputVolume: v })}
          />
          <Slider
            label={t('settings.voice.outputVolume')}
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
            {test.running ? t('settings.voice.stopTest') : t('settings.voice.startTest')}
          </Button>
          <LevelMeter db={test.level} active={test.running} label={t('settings.voice.micLevel')} />
        </div>
      </Card>

      <Card
        id="us-voice-mode"
        title={t('settings.voice.modeTitle')}
        description={t('settings.voice.modeDesc')}
      >
        <SegmentedControl
          label={t('settings.voice.modeTitle')}
          value={settings.inputMode}
          onValueChange={(v) => update({ inputMode: v === 'ptt' ? 'ptt' : 'vad' })}
          options={[
            { value: 'vad', label: t('settings.voice.vad') },
            { value: 'ptt', label: t('settings.voice.ptt') },
          ]}
        />
        {!ptt && (
          <div className="us-form-stack">
            <Switch
              label={t('settings.voice.autoSensitivity')}
              checked={settings.autoSensitivity}
              onChange={(e) => update({ autoSensitivity: e.target.checked })}
            />
            <Slider
              label={t('settings.voice.sensitivity')}
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
              label={t('settings.voice.sensitivityMeter')}
              {...(settings.autoSensitivity ? {} : { thresholdDb: settings.thresholdDb })}
            />
            <p className="us-muted">
              {test.running ? t('settings.voice.speakHint') : t('settings.voice.checkHint')}
            </p>
          </div>
        )}
        {ptt && (
          <div className="us-form-stack">
            <div className="us-row-field">
              <span id="us-ptt-key-l" className="us-label">
                {t('settings.voice.shortcut')}
              </span>
              <KeybindRecorder
                action={t('settings.voice.ptt')}
                value={settings.keybinds.ptt}
                onChange={(b) => setKeybind('ptt', b)}
                conflict={(b) => keybindConflict(settings.keybinds, 'ptt', b)}
              />
            </div>
            {!settings.keybinds.ptt && (
              <Notice variant="warning" className="us-gap">
                {t('settings.voice.pickKey')}
              </Notice>
            )}
            <Slider
              label={t('settings.voice.releaseDelay')}
              min={PTT_RELEASE_RANGE[0]}
              max={PTT_RELEASE_RANGE[1]}
              step={50}
              value={settings.pttReleaseMs}
              showValue
              formatValue={(v) => `${v} ms`}
              onValueChange={(v) => update({ pttReleaseMs: v })}
            />
            <p className="us-muted">{t('settings.voice.keybindNote')}</p>
          </div>
        )}
      </Card>

      <Card id="us-voice-processing" title={t('settings.voice.processing')}>
        <Switch
          label={t('settings.voice.echo')}
          description={t('settings.voice.echoDesc')}
          checked={settings.echoCancellation}
          onChange={(e) => update({ echoCancellation: e.target.checked })}
        />
        <Switch
          label={t('settings.voice.agc')}
          description={t('settings.voice.agcDesc')}
          checked={settings.autoGainControl}
          onChange={(e) => update({ autoGainControl: e.target.checked })}
        />
        <p className="us-muted">{t('settings.voice.noNoise')}</p>
      </Card>

      <Card
        id="us-voice-attenuation"
        title={t('settings.voice.attenuationTitle')}
        description={t('settings.voice.attenuationDesc')}
      >
        <Slider
          label={t('settings.voice.attenuation')}
          min={0}
          max={100}
          step={5}
          value={settings.attenuation}
          showValue
          formatValue={(v) => (v === 0 ? t('settings.voice.off') : pct(v))}
          onValueChange={(v) => update({ attenuation: v })}
        />
      </Card>

      <Card
        id="us-voice-camera"
        title={t('settings.voice.cameraTitle')}
        description={t('settings.voice.cameraDesc')}
      >
        <div className="us-form-grid">
          <div>
            <label id="us-camera-dev" className="us-label">
              {t('settings.voice.cameraDevice')}
            </label>
            <Select
              aria-labelledby="us-camera-dev"
              options={cameraOptions}
              value={settings.cameraDeviceId}
              onValueChange={(v) => update({ cameraDeviceId: v })}
            />
          </div>
        </div>
        <Switch
          label={t('settings.voice.alwaysPreview')}
          description={t('settings.voice.alwaysPreviewDesc')}
          checked={settings.alwaysPreviewVideo}
          onChange={(e) => update({ alwaysPreviewVideo: e.target.checked })}
        />
        <div className="us-mic-test">
          <Button
            variant="secondary"
            aria-pressed={previewOn}
            onClick={() => {
              setPreviewOn((on) => !on);
              void refreshCameras();
            }}
          >
            {previewOn ? t('settings.voice.stopPreview') : t('settings.voice.startPreview')}
          </Button>
        </div>
        {previewOn && <CameraPreview deviceId={settings.cameraDeviceId} />}
      </Card>

      <Card
        id="us-voice-share"
        title={t('settings.voice.shareTitle')}
        description={t('settings.voice.shareDesc')}
      >
        <div className="us-form-grid">
          <div>
            <label id="us-share-preset" className="us-label">
              {t('settings.voice.sharePreset')}
            </label>
            <Select
              aria-labelledby="us-share-preset"
              options={SHARE_PRESETS.map((p) => ({ value: p.id, label: presetLabel(p) }))}
              value={settings.sharePresetId}
              onValueChange={(v) => update({ sharePresetId: v })}
            />
          </div>
          <div>
            <label id="us-share-hint" className="us-label">
              {t('settings.voice.shareHint')}
            </label>
            <SegmentedControl
              label={t('settings.voice.shareHint')}
              value={settings.shareHint}
              onValueChange={(v) => update({ shareHint: v === 'detail' ? 'detail' : 'motion' })}
              options={[
                { value: 'motion', label: t('settings.voice.hintMotion') },
                { value: 'detail', label: t('settings.voice.hintDetail') },
              ]}
            />
          </div>
        </div>
        <Switch
          label={t('settings.voice.shareAudio')}
          checked={settings.shareAudio}
          onChange={(e) => update({ shareAudio: e.target.checked })}
        />
        <p className="us-muted">{t('settings.voice.shareEntitlement')}</p>
      </Card>

      <Card id="us-voice-party" title={t('settings.voice.watchPartyTitle')}>
        <Switch
          label={t('settings.voice.watchPartyCameras')}
          description={t('settings.voice.watchPartyCamerasHelp')}
          checked={prefs.watchPartyCameras}
          onChange={(e) => savePrefs({ ...prefs, watchPartyCameras: e.target.checked })}
        />
      </Card>

      <Card id="us-voice-sounds" title={t('settings.voice.sounds')}>
        <Switch
          label={t('settings.voice.soundJoinLeave')}
          checked={settings.soundJoinLeave}
          onChange={(e) => update({ soundJoinLeave: e.target.checked })}
        />
        <Switch
          label={t('settings.voice.soundMute')}
          checked={settings.soundMuteDeafen}
          onChange={(e) => update({ soundMuteDeafen: e.target.checked })}
        />
        <Switch
          label={t('settings.voice.soundPtt')}
          checked={settings.soundPtt}
          onChange={(e) => update({ soundPtt: e.target.checked })}
        />
        <Switch
          label={t('settings.voice.soundRing')}
          checked={settings.soundRing}
          onChange={(e) => update({ soundRing: e.target.checked })}
        />
        <Slider
          label={t('settings.voice.soundVolume')}
          min={0}
          max={100}
          step={5}
          value={settings.soundVolume}
          showValue
          formatValue={pct}
          onValueChange={(v) => update({ soundVolume: v })}
        />
        <Slider
          label={t('settings.voice.soundboardVolume')}
          min={0}
          max={100}
          step={5}
          value={soundboardVolume}
          showValue
          formatValue={pct}
          onValueChange={setSoundboardVolume}
        />
        <Button variant="secondary" size="sm" onClick={() => testCue('selfJoin')}>
          {t('settings.voice.playSample')}
        </Button>
      </Card>

      <Card id="us-voice-advanced" title={t('settings.voice.advancedTitle')}>
        <Notice variant="info" title={t('settings.voice.e2eeTitle')}>
          {t('settings.voice.e2eeBody')}
        </Notice>
        <p className="us-muted">{t('settings.voice.privacyCodes')}</p>
      </Card>
    </>
  );
}

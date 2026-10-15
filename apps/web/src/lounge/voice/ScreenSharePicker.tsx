import { useState } from 'react';
import { Button, Modal, Notice, Radio, RadioGroup, Switch } from '@sideroom/ui';
import { useT } from '../../i18n';
import { VIDEO_PUBLISH } from '../../voice/protocol';
import { getVoiceSettings } from '../../voice/settings';
import {
  DEFAULT_PRESET_ID,
  presetLabel,
  presetLocked,
  SHARE_PRESETS,
  type ContentHint,
  type ShareOptions,
  type ShareSource,
  type VideoProfile,
} from '../../voice/video';
import './voice-p10.css';

export interface ScreenSharePickerProps {
  open: boolean;
  onClose: () => void;
  /** The profiles the viewer may send (their account plus the server's level). */
  profiles: readonly VideoProfile[];
  /** Start the share with these choices (the browser then shows its own screen / window / tab picker). */
  onStart: (opts: ShareOptions) => void;
  /** Pre-selected values when changing a running share. */
  initial?: Partial<Omit<ShareOptions, 'preset'>> & { presetId?: string };
}

/**
 * The screen share dialog (VOX-UI-006): source type, resolution and frame rate (rows above the viewer's
 * entitlement are disabled with an upgrade hint), the share-audio switch and the content hint. The browser's
 * own picker chooses the actual screen, window or tab once "Go Live" is pressed.
 */
export function ScreenSharePicker({
  open,
  onClose,
  profiles,
  onStart,
  initial,
}: ScreenSharePickerProps) {
  const t = useT();
  // Settings > Voice & Video holds the defaults; a running share keeps what it was started with.
  const defaults = getVoiceSettings();
  const firstFree =
    SHARE_PRESETS.find(
      (p) =>
        p.id === (initial?.presetId ?? defaults.sharePresetId ?? DEFAULT_PRESET_ID) &&
        !presetLocked(p, profiles),
    ) ??
    SHARE_PRESETS.find((p) => !presetLocked(p, profiles)) ??
    SHARE_PRESETS[0]!;
  const [source, setSource] = useState<ShareSource>(initial?.source ?? 'screen');
  const [presetId, setPresetId] = useState(firstFree.id);
  const [audio, setAudio] = useState(initial?.audio ?? defaults.shareAudio);
  const [hint, setHint] = useState<ContentHint>(initial?.hint ?? defaults.shareHint);
  const preset = SHARE_PRESETS.find((p) => p.id === presetId) ?? firstFree;

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t('lounge.sharePicker.title')}
      description={t('lounge.sharePicker.description')}
      size="md"
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>
            {t('lounge.sharePicker.cancel')}
          </Button>
          <Button
            onClick={() => {
              onStart({ preset, source, audio, hint });
              onClose();
            }}
          >
            {t('lounge.sharePicker.start')}
          </Button>
        </>
      }
    >
      <div className="vc-picker">
        {!VIDEO_PUBLISH && (
          <Notice variant="info" data-testid="share-preview-only">
            {t('lounge.sharePicker.previewOnly')}
          </Notice>
        )}
        <RadioGroup
          legend={t('lounge.sharePicker.source')}
          value={source}
          onValueChange={(v) => setSource(v as ShareSource)}
        >
          <Radio value="screen" label={t('lounge.sharePicker.sourceScreen')} />
          <Radio value="window" label={t('lounge.sharePicker.sourceWindow')} />
          <Radio value="tab" label={t('lounge.sharePicker.sourceTab')} />
        </RadioGroup>
        <RadioGroup
          legend={t('lounge.sharePicker.quality')}
          value={presetId}
          onValueChange={setPresetId}
        >
          {SHARE_PRESETS.map((p) => {
            const locked = presetLocked(p, profiles);
            return (
              <Radio
                key={p.id}
                value={p.id}
                disabled={locked}
                label={presetLabel(p)}
                description={locked ? t('lounge.sharePicker.locked') : undefined}
              />
            );
          })}
        </RadioGroup>
        <RadioGroup
          legend={t('lounge.sharePicker.hint')}
          value={hint}
          onValueChange={(v) => setHint(v as ContentHint)}
        >
          <Radio
            value="motion"
            label={t('lounge.sharePicker.hintMotion')}
            description={t('lounge.sharePicker.hintMotionHelp')}
          />
          <Radio
            value="detail"
            label={t('lounge.sharePicker.hintDetail')}
            description={t('lounge.sharePicker.hintDetailHelp')}
          />
        </RadioGroup>
        <Switch
          label={t('lounge.sharePicker.audio')}
          description={t('lounge.sharePicker.audioHelp')}
          checked={audio}
          onChange={(e) => setAudio(e.target.checked)}
        />
      </div>
    </Modal>
  );
}

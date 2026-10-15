import { useState } from 'react';
import { DropdownMenu, Icon, Tooltip, type MenuEntry } from '@sideroom/ui';
import { useT } from '../../i18n';
import { useOpenUserSettings } from '../../settings/user/nav';
import { voiceController } from '../../voice/controller';
import { useAudioDevices, useCameras } from '../../voice/hooks';
import { useVideoLimits } from '../../voice/limits';
import { useVoiceSession } from '../../voice/session';
import { useVoiceSettings } from '../../voice/settings';
import { useLocalVideo } from '../../voice/video';
import { StartPartyButton } from '../../watchparty/StartPartyButton';
import { CameraPreviewDialog } from './CameraPreview';
import { ScreenSharePicker } from './ScreenSharePicker';
import './voice-p10.css';

export interface ViewLayout {
  mode: 'grid' | 'focus';
  hideNonVideo: boolean;
  chatOpen: boolean;
}

export interface CallControlsProps {
  channelId: string;
  guildId: string;
  layout: ViewLayout;
  onLayout(patch: Partial<ViewLayout>): void;
  onFullscreen(): void;
  onPopOut?: (() => void) | undefined;
}

/** A round toggle with a drop-down arrow beside it. */
function Split({
  main,
  menu,
  menuLabel,
}: {
  main: React.ReactNode;
  menu: MenuEntry[];
  menuLabel: string;
}) {
  return (
    <span className="vc-split">
      {main}
      <DropdownMenu
        label={menuLabel}
        placement="top"
        items={menu}
        trigger={
          <button type="button" className="vc-ctl-arrow" aria-label={menuLabel}>
            <Icon name="chevron-up" size={16} />
          </button>
        }
      />
    </span>
  );
}

/**
 * The bottom call controls (VOX-UI-005): mic with an input-device menu, deafen, camera with a camera menu,
 * share screen with quality options, text chat, watch party, more (layout, full screen) and disconnect.
 */
export function CallControls({
  channelId,
  guildId,
  layout,
  onLayout,
  onFullscreen,
  onPopOut,
}: CallControlsProps) {
  const t = useT();
  const session = useVoiceSession();
  const inputDeviceId = useVoiceSettings((s) => s.settings.inputDeviceId);
  const update = useVoiceSettings((s) => s.update);
  const savedCamera = useVoiceSettings((s) => s.settings.cameraDeviceId);
  const alwaysPreview = useVoiceSettings((s) => s.settings.alwaysPreviewVideo);
  const [previewing, setPreviewing] = useState(false);
  const openSettings = useOpenUserSettings();
  const { inputs } = useAudioDevices();
  const { cameras } = useCameras();
  const limits = useVideoLimits(guildId);
  const screen = useLocalVideo((s) => s.screen);
  const camera = useLocalVideo((s) => s.camera);
  const videoError = useLocalVideo((s) => s.error);
  const [picker, setPicker] = useState(false);
  const controller = voiceController();
  const mutedNow = session.selfMute || session.serverMute;
  const deafNow = session.selfDeaf || session.serverDeaf;

  const cameraDevice = camera?.deviceId ?? savedCamera;
  const startCamera = (deviceId: string) => void controller.startCamera(limits.profiles, deviceId);
  const named = (d: MediaDeviceInfo) => ({
    value: d.deviceId,
    label: d.label || t('lounge.callControls.unnamedDevice'),
  });

  const micMenu: MenuEntry[] = [
    {
      type: 'radio-group',
      label: t('lounge.callControls.inputDevice'),
      value: inputDeviceId,
      onValueChange: (v) => update({ inputDeviceId: v }),
      options: [
        { value: 'default', label: t('lounge.callControls.defaultDevice') },
        ...inputs.filter((d) => d.deviceId && d.deviceId !== 'default').map(named),
      ],
    },
    { type: 'separator' },
    { label: t('lounge.callControls.voiceSettings'), onSelect: () => openSettings('voice') },
  ];
  const cameraMenu: MenuEntry[] = cameras.length
    ? [
        {
          type: 'radio-group',
          label: t('lounge.callControls.camera'),
          value: cameraDevice,
          onValueChange: (v) => {
            update({ cameraDeviceId: v });
            if (camera) startCamera(v);
          },
          options: [
            { value: 'default', label: t('lounge.callControls.defaultDevice') },
            ...cameras.filter((d) => d.deviceId && d.deviceId !== 'default').map(named),
          ],
        },
      ]
    : [{ label: t('lounge.callControls.noCameras'), disabled: true }];
  const shareMenu: MenuEntry[] = screen
    ? [
        { label: t('lounge.callControls.changeQuality'), onSelect: () => setPicker(true) },
        {
          type: 'radio-group',
          label: t('lounge.callControls.contentHint'),
          value: screen.hint,
          onValueChange: (v) => controller.setShareHint(v === 'detail' ? 'detail' : 'motion'),
          options: [
            { value: 'motion', label: t('lounge.sharePicker.hintMotion') },
            { value: 'detail', label: t('lounge.sharePicker.hintDetail') },
          ],
        },
        { type: 'separator' },
        {
          label: t('lounge.callControls.stopSharing'),
          danger: true,
          onSelect: () => controller.stopScreenShare(),
        },
      ]
    : [{ label: t('lounge.callControls.shareOptions'), onSelect: () => setPicker(true) }];
  const moreMenu: MenuEntry[] = [
    {
      type: 'radio-group',
      label: t('lounge.callControls.layout'),
      value: layout.mode,
      onValueChange: (v) => onLayout({ mode: v === 'focus' ? 'focus' : 'grid' }),
      options: [
        { value: 'grid', label: t('lounge.callControls.layoutGrid') },
        { value: 'focus', label: t('lounge.callControls.layoutFocus') },
      ],
    },
    {
      type: 'checkbox',
      label: t('lounge.callControls.hideNonVideo'),
      checked: layout.hideNonVideo,
      onCheckedChange: (on) => onLayout({ hideNonVideo: on }),
    },
    { type: 'separator' },
    { label: t('lounge.callControls.fullscreen'), onSelect: onFullscreen },
    ...(onPopOut
      ? [{ label: t('lounge.callControls.popOut'), onSelect: onPopOut } as MenuEntry]
      : []),
  ];

  return (
    <>
      {videoError && (
        <p role="alert" className="vc-inline-error vc-controls-error">
          {videoError}{' '}
          <button type="button" className="vc-link" onClick={() => controller.dismissVideoError()}>
            {t('lounge.callControls.dismiss')}
          </button>
        </p>
      )}
      <div
        className="vc-controls"
        role="toolbar"
        aria-label={t('lounge.voiceChannel.callControls')}
      >
        <Split
          menuLabel={t('lounge.callControls.micMenu')}
          menu={micMenu}
          main={
            <button
              type="button"
              className="vc-ctl"
              aria-label={
                mutedNow ? t('lounge.voiceChannel.unmute') : t('lounge.voiceChannel.mute')
              }
              aria-pressed={mutedNow}
              disabled={session.serverMute}
              data-on={mutedNow || undefined}
              onClick={() => controller.toggleMute()}
            >
              <Icon name={mutedNow ? 'mic-off' : 'mic'} size={24} />
            </button>
          }
        />
        <button
          type="button"
          className="vc-ctl"
          aria-label={deafNow ? t('lounge.voiceChannel.undeafen') : t('lounge.voiceChannel.deafen')}
          aria-pressed={deafNow}
          disabled={session.serverDeaf}
          data-on={deafNow || undefined}
          onClick={() => controller.toggleDeafen()}
        >
          <Icon name={deafNow ? 'volume-off' : 'headphones'} size={24} />
        </button>
        <Split
          menuLabel={t('lounge.callControls.cameraMenu')}
          menu={cameraMenu}
          main={
            <button
              type="button"
              className="vc-ctl"
              aria-label={
                session.camera
                  ? t('lounge.callControls.cameraOff')
                  : t('lounge.callControls.cameraOn')
              }
              aria-pressed={session.camera}
              data-active={session.camera || undefined}
              onClick={() =>
                session.camera
                  ? controller.stopCamera()
                  : alwaysPreview
                    ? setPreviewing(true)
                    : startCamera(cameraDevice)
              }
            >
              <Icon name={session.camera ? 'video' : 'video-off'} size={24} />
            </button>
          }
        />
        <Split
          menuLabel={t('lounge.callControls.shareMenu')}
          menu={shareMenu}
          main={
            <button
              type="button"
              className="vc-ctl"
              aria-label={
                session.screen
                  ? t('lounge.callControls.stopSharing')
                  : t('lounge.callControls.share')
              }
              aria-pressed={session.screen}
              data-active={session.screen || undefined}
              onClick={() => (session.screen ? controller.stopScreenShare() : setPicker(true))}
            >
              <Icon name="screen-share" size={24} />
            </button>
          }
        />
        <Tooltip content={t('lounge.callControls.textChat')}>
          <button
            type="button"
            className="vc-ctl"
            aria-label={t('lounge.callControls.textChat')}
            aria-pressed={layout.chatOpen}
            data-active={layout.chatOpen || undefined}
            onClick={() => onLayout({ chatOpen: !layout.chatOpen })}
          >
            <Icon name="message" size={24} />
          </button>
        </Tooltip>
        <StartPartyButton channelId={channelId} />
        <DropdownMenu
          label={t('lounge.callControls.more')}
          placement="top"
          items={moreMenu}
          trigger={
            <button
              type="button"
              className="vc-ctl"
              aria-label={t('lounge.callControls.moreOptions')}
            >
              <Icon name="more-horizontal" size={24} />
            </button>
          }
        />
        <button
          type="button"
          className="vc-ctl vc-ctl-danger"
          aria-label={t('lounge.voiceChannel.disconnect')}
          onClick={() => controller.leave()}
        >
          <Icon name="logout" size={24} />
        </button>
      </div>
      <CameraPreviewDialog
        open={previewing}
        deviceId={cameraDevice}
        onClose={() => setPreviewing(false)}
        onStart={() => startCamera(cameraDevice)}
      />
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
    </>
  );
}

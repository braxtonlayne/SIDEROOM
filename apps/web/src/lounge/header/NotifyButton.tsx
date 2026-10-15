import { DropdownMenu, Icon, type MenuEntry } from '@sideroom/ui';
import { useEffect } from 'react';
import { useT } from '../../i18n';
import { useLoungeUi } from '../store';
import { isMuted, MUTE_DURATIONS, useNotifSettings } from '../shell/notifSettings';

/** Notification bell menu in the channel header: mute durations and the settings dialog (LNG-CHN-018). */
export function NotifyButton({ guildId, channelId }: { guildId: string; channelId: string }) {
  const t = useT();
  const notif = useNotifSettings((s) => s.byGuild[guildId]);
  const patchChannel = useNotifSettings((s) => s.patchChannel);
  const openDialog = useLoungeUi((s) => s.openDialog);
  useEffect(() => {
    void useNotifSettings.getState().load(guildId);
  }, [guildId]);
  const muted = isMuted(notif?.channels[channelId]);
  const run = (p: Promise<void>) => void p.catch(() => undefined);

  const items: MenuEntry[] = [
    muted
      ? {
          label: t('lounge.notifyButton.unmute'),
          onSelect: () => run(patchChannel(guildId, channelId, { mute: { muted: false } })),
        }
      : {
          type: 'submenu',
          label: t('lounge.notifyButton.mute'),
          items: MUTE_DURATIONS.map((d) => ({
            label: d.label,
            onSelect: () =>
              run(patchChannel(guildId, channelId, { mute: { muted: true, seconds: d.seconds } })),
          })),
        },
    {
      label: t('lounge.notifyButton.settings'),
      onSelect: () => openDialog({ kind: 'notifications', guildId, channelId }),
    },
  ];
  const label = t('lounge.channelHeader.notificationSettings');
  return (
    <DropdownMenu
      label={label}
      placement="bottom-end"
      items={items}
      trigger={
        <button
          type="button"
          className="lng-icon-btn"
          aria-label={label}
          title={muted ? t('lounge.notifyButton.mutedTitle') : label}
          data-muted={muted || undefined}
        >
          <Icon name="bell" size={20} />
        </button>
      }
    />
  );
}

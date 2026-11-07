import type { MenuEntry } from '@sideroom/ui';
import { t } from '../../i18n';
import { useStore } from '../../store';
import { useLoungeActions, type Channel, type Guild } from '../data';
import { useDeveloperMode } from '../../settings/user/prefs';
import { useLoungeUi } from '../store';
import { applyChannelMoves } from './channelActions';
import { canStep, planStep } from './channelMoves';
import { entryList, useReportEntry } from '../../moderation/reports';
import { isMuted, MUTE_DURATIONS, useNotifSettings } from './notifSettings';

const TYPE_NUM: Record<Channel['type'], number> = {
  text: 0,
  voice: 2,
  category: 4,
  announcement: 5,
  stage: 13,
  forum: 15,
  media: 16,
  rules: 0,
};

/** Channel kinds a slowmode preset can be applied to. */
const SLOWMODE_KINDS = new Set<Channel['type']>([
  'text',
  'announcement',
  'forum',
  'media',
  'voice',
]);

/** Context-menu entries for a channel or category (LNG-CHN-013, LNG-CHN-014). */
export function useChannelMenu(
  channel: Channel,
  guild: Guild,
  channels: Channel[],
  onError: (message: string) => void,
): MenuEntry[] {
  const developerMode = useDeveloperMode();
  const reportEntry = useReportEntry();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const toggleCategory = useLoungeUi((s) => s.toggleCategory);
  const collapsed = useLoungeUi((s) => !!s.collapsedCategories[channel.id]);
  const actions = useLoungeActions();
  const notif = useNotifSettings((s) => s.byGuild[guild.id]);
  const patchChannel = useNotifSettings((s) => s.patchChannel);
  // Channel-level bits (overwrites applied) win over the guild-level fallback (D-178).
  const manage = channel.permissions?.manageChannels ?? guild.permissions.manageChannels;
  const cat = channel.type === 'category';
  const muted = isMuted(notif?.channels[channel.id]);
  const fail = () => onError(t('lounge.menus.saveFailed'));

  const move = (delta: -1 | 1) => {
    const moves = planStep(channels, channel.id, delta);
    if (moves)
      applyChannelMoves(guild.id, moves).catch(() => onError(t('lounge.menus.moveFailed')));
  };

  const muteEntry: MenuEntry = muted
    ? {
        label: cat ? t('lounge.menus.unmuteCategory') : t('lounge.menus.unmuteChannel'),
        onSelect: () =>
          void patchChannel(guild.id, channel.id, { mute: { muted: false } }).catch(fail),
      }
    : {
        type: 'submenu',
        label: cat ? t('lounge.menus.muteCategory') : t('lounge.menus.muteChannel'),
        items: MUTE_DURATIONS.map((d) => ({
          label: d.label,
          onSelect: () =>
            void patchChannel(guild.id, channel.id, {
              mute: { muted: true, seconds: d.seconds },
            }).catch(fail),
        })),
      };

  const unreadIds = cat
    ? channels.filter((c) => c.parentId === channel.id && c.unread).map((c) => c.id)
    : channel.unread
      ? [channel.id]
      : [];

  const markRead: MenuEntry = {
    label: t('lounge.menus.markRead'),
    disabled: unreadIds.length === 0,
    onSelect: () => unreadIds.forEach((id) => actions.markRead(id)),
  };
  const notifications: MenuEntry = {
    label: t('lounge.menus.notificationSettings'),
    onSelect: () => openDialog({ kind: 'notifications', guildId: guild.id, channelId: channel.id }),
  };
  const moveUp: MenuEntry = {
    label: t('lounge.menus.moveUp'),
    disabled: !manage || !canStep(channels, channel.id, -1),
    onSelect: () => move(-1),
  };
  const moveDown: MenuEntry = {
    label: t('lounge.menus.moveDown'),
    disabled: !manage || !canStep(channels, channel.id, 1),
    onSelect: () => move(1),
  };
  const remove: MenuEntry = {
    label: cat ? t('lounge.menus.deleteCategory') : t('lounge.menus.deleteChannel'),
    danger: true,
    disabled: !manage,
    onSelect: () => openDialog({ kind: 'deleteChannel', guildId: guild.id, channelId: channel.id }),
  };
  const edit: MenuEntry = {
    label: cat ? t('lounge.menus.editCategory') : t('lounge.menus.editChannel'),
    disabled: !manage,
    onSelect: () => openDialog({ kind: 'editChannel', guildId: guild.id, channelId: channel.id }),
  };

  if (cat) {
    return [
      markRead,
      {
        label: collapsed ? t('lounge.menus.expandCategory') : t('lounge.menus.collapseCategory'),
        onSelect: () => toggleCategory(channel.id),
      },
      {
        label: t('lounge.menus.collapseAll'),
        onSelect: () => {
          const st = useLoungeUi.getState();
          for (const c of channels)
            if (c.type === 'category' && !st.collapsedCategories[c.id]) st.toggleCategory(c.id);
        },
      },
      muteEntry,
      notifications,
      { type: 'separator' },
      {
        label: t('lounge.menus.createChannel'),
        disabled: !manage,
        onSelect: () =>
          openDialog({ kind: 'createChannel', guildId: guild.id, parentId: channel.id }),
      },
      edit,
      moveUp,
      moveDown,
      remove,
      ...(developerMode
        ? [
            { type: 'separator' } as MenuEntry,
            {
              label: t('lounge.menus.copyCategoryId'),
              onSelect: () => void navigator.clipboard?.writeText(channel.id),
            },
          ]
        : []),
    ];
  }

  return [
    markRead,
    {
      label: t('lounge.menus.copyLink'),
      onSelect: () =>
        void navigator.clipboard?.writeText(
          `${location.origin}/channels/${guild.id}/${channel.id}`,
        ),
    },
    muteEntry,
    notifications,
    { type: 'separator' },
    ...(guild.isStaff
      ? []
      : [
          {
            label: t('lounge.menus.invitePeople'),
            disabled: !(channel.permissions?.createInvite ?? guild.permissions.createInvite),
            onSelect: () =>
              openDialog({ kind: 'invite', guildId: guild.id, channelId: channel.id }),
          } as MenuEntry,
        ]),
    edit,
    {
      label: t('lounge.menus.duplicateChannel'),
      disabled: !manage,
      onSelect: () =>
        void useStore
          .getState()
          .createChannel(guild.id, {
            name: channel.name,
            type: TYPE_NUM[channel.type],
            ...(channel.parentId ? { parent_id: channel.parentId } : {}),
            ...(channel.topic ? { topic: channel.topic } : {}),
          })
          .catch(() => onError(t('lounge.menus.duplicateFailed'))),
    },
    {
      label: t('lounge.menus.createChannel'),
      disabled: !manage,
      onSelect: () =>
        openDialog({
          kind: 'createChannel',
          guildId: guild.id,
          ...(channel.parentId ? { parentId: channel.parentId } : {}),
        }),
    },
    // Slowmode presets applied on demand (G-74): for channels that take messages, with Manage Channels.
    ...(manage && SLOWMODE_KINDS.has(channel.type)
      ? [
          {
            label: t('lounge.menus.slowmodePreset'),
            onSelect: () =>
              openDialog({ kind: 'slowmode', guildId: guild.id, channelId: channel.id }),
          } as MenuEntry,
        ]
      : []),
    moveUp,
    moveDown,
    remove,
    ...entryList(reportEntry({ kind: 'channel', id: channel.id })),
    ...(developerMode
      ? [
          { type: 'separator' } as MenuEntry,
          {
            label: t('lounge.menus.copyChannelId'),
            onSelect: () => void navigator.clipboard?.writeText(channel.id),
          },
        ]
      : []),
  ];
}

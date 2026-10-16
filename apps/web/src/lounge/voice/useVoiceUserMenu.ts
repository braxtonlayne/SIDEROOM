import type { MenuEntry } from '@sideroom/ui';
import { useT } from '../../i18n';
import { useDeveloperMode } from '../../settings/user/prefs';
import { useViewer } from '../../settings/server/hooks';
import { DM_GUILD_ID, voiceController } from '../../voice/controller';
import { setVideoOff, useRemoteVideo } from '../../voice/video';
import { useGuild, useMembers, useRoles } from '../data';
import { openProfilePopout } from '../profile/popoutStore';
import { useOpenDm } from '../shell/dm';
import { useUserMenuEntries } from '../shell/userMenu';
import { useLoungeUi } from '../store';
import { useVoiceMenuEntries } from './useVoiceMenu';

/**
 * The full right-click menu of a person in a voice channel (VOX-UI-008): Profile, Mention, Message, Add Note,
 * the voice entries (User Volume, Mute, Server Mute / Deafen, Move To, Disconnect), Timeout / Kick / Ban and
 * Copy User ID. The server re-checks every permission and the role hierarchy; this only hides what would
 * certainly be refused. Returns nothing for the viewer themselves.
 *
 * Call opens the DM with the person and joins its call (the server rings them, VOX-SIG-008); Disable Video
 * stops receiving their camera for this call (VOX-SFU-005: the SFU stops forwarding it).
 */
export function useVoiceUserMenu(
  guildId: string,
  user: { id: string; name: string },
  onError?: (message: string) => void,
  channelHint?: string,
  /** Where the profile popout anchors; without one the full profile opens. */
  anchor?: HTMLElement | null,
): MenuEntry[] {
  const t = useT();
  const developerMode = useDeveloperMode();
  const voice = useVoiceMenuEntries(guildId, user, onError, channelHint);
  const viewer = useViewer(guildId);
  const guild = useGuild(guildId);
  const roles = useRoles(guildId);
  const members = useMembers(guildId);
  const openDm = useOpenDm();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const userEntries = useUserMenuEntries(voice.length ? user : undefined);
  const hasCamera = useRemoteVideo((s) => !!s.camera[user.id]);
  const videoOff = useRemoteVideo((s) => !!s.videoOff[user.id]);
  if (!voice.length) return [];

  const member = members.find((m) => m.user.id === user.id);
  const isBot = !!member?.user.bot;
  const targetTop = Math.max(
    -1,
    ...(member?.roleIds ?? []).map((id) => roles.find((r) => r.id === id)?.position ?? -1),
  );
  const isOwner = guild?.ownerId === user.id;
  const canAct = !isOwner && (viewer.isOwner || viewer.topPosition > targetTop);
  const act = (action: 'timeout' | 'kick' | 'ban') => () =>
    openDialog({ kind: 'memberAction', action, guildId, userId: user.id });

  const moderation: MenuEntry[] = [
    ...(viewer.can('MODERATE_MEMBERS') && canAct
      ? [{ label: t('lounge.memberList.timeout'), onSelect: act('timeout') } as MenuEntry]
      : []),
    ...(viewer.can('KICK_MEMBERS') && canAct
      ? [
          {
            label: t('lounge.memberList.kick', { name: user.name }),
            danger: true,
            onSelect: act('kick'),
          } as MenuEntry,
        ]
      : []),
    ...(viewer.can('BAN_MEMBERS') && canAct
      ? [
          {
            label: t('lounge.memberList.ban', { name: user.name }),
            danger: true,
            onSelect: act('ban'),
          } as MenuEntry,
        ]
      : []),
  ];

  return [
    {
      label: t('lounge.memberList.profile'),
      onSelect: () => openProfilePopout({ userId: user.id, guildId }, anchor ?? null),
    },
    {
      label: t('lounge.memberList.mention'),
      onSelect: () => void navigator.clipboard?.writeText(`<@${user.id}>`),
    },
    {
      label: t('lounge.memberList.message'),
      disabled: isBot,
      onSelect: () => void openDm([user.id]).catch(() => onError?.(t('lounge.voiceMenu.dmFailed'))),
    },
    {
      label: t('lounge.voiceMenu.call'),
      disabled: isBot,
      onSelect: () =>
        void openDm([user.id])
          .then((dm) =>
            voiceController().join({ guildId: DM_GUILD_ID, channelId: dm.id, name: user.name }),
          )
          .catch(() => onError?.(t('lounge.voiceMenu.dmFailed'))),
    },
    ...(userEntries.note ? [userEntries.note] : []),
    { type: 'separator' },
    ...voice,
    ...(hasCamera || videoOff
      ? [
          {
            label: videoOff
              ? t('lounge.voiceChannel.enableVideo')
              : t('lounge.voiceChannel.disableVideo'),
            onSelect: () => setVideoOff(user.id, !videoOff),
          } as MenuEntry,
        ]
      : []),
    ...(moderation.length ? [{ type: 'separator' } as MenuEntry, ...moderation] : []),
    ...(developerMode
      ? [
          { type: 'separator' } as MenuEntry,
          {
            label: t('lounge.memberList.copyUserId'),
            onSelect: () => void navigator.clipboard?.writeText(user.id),
          },
        ]
      : []),
  ];
}

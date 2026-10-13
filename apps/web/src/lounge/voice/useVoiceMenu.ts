import type { MenuEntry } from '@sideroom/ui';
import { useStore } from '../../store/store';
import { useVoiceSettings, userVolumeOf } from '../../voice/settings';
import { useChannels } from '../data';
import { moderateVoiceState } from './actions';
import { useVoicePerms } from './perms';

export const VOLUME_PRESETS = [0, 25, 50, 75, 100, 125, 150, 175, 200] as const;

/**
 * Context-menu entries for a person who is in a voice channel (VOX-UI-008): User Volume, local Mute, and,
 * for moderators, Server Mute / Deafen, Move To and Disconnect. Returns nothing for people not in voice and
 * for the viewer themselves (they use the voice panel). The server re-checks every permission.
 */
export function useVoiceMenuEntries(
  guildId: string,
  user: { id: string; name: string },
  onError?: (message: string) => void,
  /** The voice channel the caller already knows they are in (the store may not have their state). */
  channelHint?: string,
): MenuEntry[] {
  const state = useStore((s) => s.voiceStates[guildId]?.[user.id]);
  const me = useStore((s) => s.currentUserId);
  const channels = useChannels(guildId);
  const channelId = state?.channel_id ?? channelHint;
  const perms = useVoicePerms(guildId, channelId);
  const volume = useVoiceSettings((s) => userVolumeOf(s.settings, user.id));
  const localMuted = useVoiceSettings((s) => !!s.settings.localMuted[user.id]);
  const setVolume = useVoiceSettings((s) => s.setUserVolume);
  const setLocalMuted = useVoiceSettings((s) => s.setLocalMuted);

  if (!channelId || user.id === me) return [];
  const fail = (e: unknown) =>
    onError?.(e instanceof Error ? e.message : 'The voice change was refused.');
  const act = (body: Parameters<typeof moderateVoiceState>[2]) =>
    void moderateVoiceState(guildId, user.id, body).catch(fail);

  const entries: MenuEntry[] = [
    {
      type: 'submenu',
      label: 'User Volume',
      items: [
        {
          type: 'radio-group',
          label: `${user.name}'s volume`,
          value: String(volume),
          onValueChange: (v) => setVolume(user.id, Number(v)),
          options: VOLUME_PRESETS.map((p) => ({ value: String(p), label: `${p}%` })),
        },
      ],
    },
    {
      type: 'checkbox',
      label: 'Mute',
      checked: localMuted,
      onCheckedChange: (on) => setLocalMuted(user.id, on),
    },
  ];

  const moderating = perms.muteMembers || perms.deafenMembers || perms.moveMembers;
  if (moderating) {
    entries.push({ type: 'separator' });
    if (perms.muteMembers)
      entries.push({
        type: 'checkbox',
        label: 'Server Mute',
        checked: !!state?.mute,
        onCheckedChange: (on) => act({ mute: on }),
      });
    if (perms.deafenMembers)
      entries.push({
        type: 'checkbox',
        label: 'Server Deafen',
        checked: !!state?.deaf,
        onCheckedChange: (on) => act({ deaf: on }),
      });
    if (perms.moveMembers) {
      const targets = channels.filter((c) => c.type === 'voice' && c.id !== channelId);
      entries.push({
        type: 'submenu',
        label: 'Move To',
        items: targets.length
          ? targets.map((c) => ({ label: c.name, onSelect: () => act({ channel_id: c.id }) }))
          : [{ label: 'No other voice channels', disabled: true }],
      });
      entries.push({
        label: 'Disconnect',
        danger: true,
        onSelect: () => act({ channel_id: null }),
      });
    }
  }
  return entries;
}

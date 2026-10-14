import { useNavigate } from '@tanstack/react-router';
import type { MenuEntry } from '@sideroom/ui';
import { useEffect } from 'react';
import { t } from '../../i18n';
import { useCurrentUser, useLoungeActions, type Guild } from '../data';
import { useLoungeUi } from '../store';
import { isMuted, MUTE_DURATIONS, useNotifSettings } from './notifSettings';

/** Loads the person's notification settings for each server once (feeds mute dimming on the rail). */
export function useLoadNotifSettings(guildIds: string[]) {
  const load = useNotifSettings((s) => s.load);
  const key = guildIds.join(',');
  useEffect(() => {
    for (const id of key ? key.split(',') : []) void load(id);
  }, [key, load]);
}

/** "Mute Server" with its durations, or "Unmute Server" when already muted. */
export function useMuteEntry(guildId: string): MenuEntry {
  const notif = useNotifSettings((s) => s.byGuild[guildId]);
  const patch = useNotifSettings((s) => s.patchGuild);
  if (isMuted(notif))
    return {
      label: 'Unmute Server',
      onSelect: () => void patch(guildId, { mute: { muted: false } }).catch(() => undefined),
    };
  return {
    type: 'submenu',
    label: 'Mute Server',
    items: MUTE_DURATIONS.map((d) => ({
      label: d.label,
      onSelect: () =>
        void patch(guildId, { mute: { muted: true, seconds: d.seconds } }).catch(() => undefined),
    })),
  };
}

/**
 * Entries shared by the rail context menu and the server header dropdown (LNG-RAIL-012, LNG-GLD-020):
 * everything the backend already supports is real; the rest stays in the header menu as coming-soon.
 */
export function useGuildMenuCore(guild: Guild) {
  const me = useCurrentUser();
  const actions = useLoungeActions();
  const navigate = useNavigate();
  const openDialog = useLoungeUi((s) => s.openDialog);
  const hideMuted = useLoungeUi((s) => !!s.hideMutedChannels[guild.id]);
  const toggleHideMuted = useLoungeUi((s) => s.toggleHideMuted);
  const mute = useMuteEntry(guild.id);
  const p = guild.permissions;
  const isOwner = guild.ownerId === me.id;
  return {
    markRead: {
      label: 'Mark As Read',
      disabled: !guild.unread,
      onSelect: () => actions.markGuildRead(guild.id),
    } as MenuEntry,
    invite: {
      label: 'Invite People',
      disabled: !p.createInvite,
      onSelect: () => openDialog({ kind: 'invite', guildId: guild.id }),
    } as MenuEntry,
    mute,
    notifications: {
      label: 'Notification Settings',
      onSelect: () => openDialog({ kind: 'notifications', guildId: guild.id }),
    } as MenuEntry,
    hideMuted: {
      type: 'checkbox',
      label: 'Hide Muted Channels',
      checked: hideMuted,
      onCheckedChange: () => toggleHideMuted(guild.id),
    } as MenuEntry,
    settings: {
      label: 'Server Settings',
      disabled: !(p.serverSettings ?? p.manageGuild),
      onSelect: () =>
        void navigate({ to: '/settings/server/$guildId', params: { guildId: guild.id } }),
    } as MenuEntry,
    createChannel: {
      label: 'Create Channel',
      disabled: !p.manageChannels,
      onSelect: () => openDialog({ kind: 'createChannel', guildId: guild.id }),
    } as MenuEntry,
    createCategory: {
      label: 'Create Category',
      disabled: !p.manageChannels,
      onSelect: () => openDialog({ kind: 'createChannel', guildId: guild.id, category: true }),
    } as MenuEntry,
    leave: {
      label: 'Leave Server',
      danger: true,
      // The owner must transfer ownership first (LNG-GLD-027); the dialog explains it.
      onSelect: () => openDialog({ kind: 'leaveServer', guildId: guild.id }),
    } as MenuEntry,
    copyId: {
      label: 'Copy Server ID',
      onSelect: () => void navigator.clipboard?.writeText(guild.id),
    } as MenuEntry,
    platformAdmin: {
      label: t('lounge.platformAdmin.menu'),
      onSelect: () => void navigate({ to: '/admin' }),
    } as MenuEntry,
    isOwner,
  };
}

import type { MenuEntry } from '@sideroom/ui';
import { rest } from '../../store';
import { isTextLike } from '../channels';
import { useAllChannels, useGuilds } from '../data';
import { inviteUrl } from '../inviteFlow';
import { useLoungeUi } from '../store';
import { blockUser, createDm, ignoreUser, removeRelationship } from './api';
import { rememberDm } from './dm';
import { friendsOf, useSocial } from './social';
import { entryList, useReportEntry } from '../../moderation/reports';

/**
 * DM an invite to a channel's server to one person. The invite never leaves the platform, so it uses the
 * server defaults (7 days, unlimited) unless the caller passes options (D-182).
 */
export async function sendServerInvite(
  channelId: string,
  userId: string,
  opts: rest.CreateInviteOptions = {},
): Promise<void> {
  const inv = await rest.createInvite(channelId, opts);
  const dm = await createDm([userId]);
  rememberDm(dm);
  await rest.postMessage(dm.id, inviteUrl(inv.code), `inv${Date.now().toString(36)}${userId}`);
}

/**
 * Entries about another person that DM rows, the member list and the friends page share: Add Note,
 * Invite to Server, and the relationship actions (LNG-HOME-008, LNG-GLD-023).
 */
export function useUserMenuEntries(user: { id: string; name: string } | undefined) {
  const guilds = useGuilds();
  const channelsByGuild = useAllChannels();
  const relationships = useSocial((s) => s.relationships);
  const reportEntry = useReportEntry();
  const openDialog = useLoungeUi((s) => s.openDialog);
  if (!user) return { note: null, invite: null, relationship: [] as MenuEntry[] };
  const isFriend = friendsOf(relationships).some((r) => r.id === user.id);
  // The gateway (RELATIONSHIP_*) updates the friend lists, so nothing is re-fetched here (D-183).
  const after = (req: Promise<unknown>) => void req.catch(() => undefined);

  const targets = guilds
    .filter((g) => g.permissions.createInvite)
    .map((g) => ({ g, ch: (channelsByGuild[g.id] ?? []).find(isTextLike) }))
    .filter((t): t is { g: (typeof guilds)[number]; ch: NonNullable<typeof t.ch> } => !!t.ch);

  const note: MenuEntry = {
    label: 'Add Note',
    onSelect: () => openDialog({ kind: 'note', userId: user.id, name: user.name }),
  };
  const invite: MenuEntry = {
    type: 'submenu',
    label: 'Invite to Server',
    items: targets.length
      ? targets.map(({ g, ch }) => ({
          label: g.name,
          onSelect: () => void sendServerInvite(ch.id, user.id).catch(() => undefined),
        }))
      : [{ label: 'No servers to invite to', disabled: true }],
  };
  const relationship: MenuEntry[] = [
    ...(isFriend
      ? [
          {
            label: 'Remove Friend',
            danger: true,
            onSelect: () => after(removeRelationship(user.id)),
          } as MenuEntry,
        ]
      : []),
    { label: 'Ignore', onSelect: () => after(ignoreUser(user.id)) },
    { label: 'Block', danger: true, onSelect: () => after(blockUser(user.id)) },
    ...entryList(reportEntry({ kind: 'user', id: user.id })),
  ];
  return { note, invite, relationship };
}

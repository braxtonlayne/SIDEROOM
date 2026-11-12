import type { MenuEntry } from '@sideroom/ui';
import { t, type MessageKey } from '../../i18n';
import { rest } from '../../store';
import { failWith } from '../actionError';
import { isTextLike } from '../channels';
import { useAllChannels, useGuilds } from '../data';
import { inviteUrl } from '../inviteFlow';
import { useLoungeUi } from '../store';
import { createDm, ignoreUser } from './api';
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
export function useUserMenuEntries(
  user: { id: string; name: string } | undefined,
  /** The server the menu was opened in, so its moderators get a report about this person. */
  guildId?: string,
) {
  const guilds = useGuilds();
  const channelsByGuild = useAllChannels();
  const relationships = useSocial((s) => s.relationships);
  const reportEntry = useReportEntry();
  const openDialog = useLoungeUi((s) => s.openDialog);
  if (!user) return { note: null, invite: null, relationship: [] as MenuEntry[] };
  const isFriend = friendsOf(relationships).some((r) => r.id === user.id);
  // The gateway (RELATIONSHIP_*) updates the friend lists, so nothing is re-fetched here (D-183).
  // A failure is shown as a toast: the menu has closed, so nothing else would tell the person (SEC-AUD-036).
  const after = (req: Promise<unknown>, failure: MessageKey) => void req.catch(failWith(failure));

  const targets = guilds
    .filter((g) => g.permissions.createInvite)
    .map((g) => ({ g, ch: (channelsByGuild[g.id] ?? []).find(isTextLike) }))
    .filter((t): t is { g: (typeof guilds)[number]; ch: NonNullable<typeof t.ch> } => !!t.ch);

  const note: MenuEntry = {
    label: t('lounge.menus.addNote'),
    onSelect: () => openDialog({ kind: 'note', userId: user.id, name: user.name }),
  };
  const invite: MenuEntry = {
    type: 'submenu',
    label: t('lounge.menus.inviteToServer'),
    items: targets.length
      ? targets.map(({ g, ch }) => ({
          label: g.name,
          onSelect: () =>
            void sendServerInvite(ch.id, user.id).catch(failWith('lounge.actionError.invite')),
        }))
      : [{ label: t('lounge.menus.noServersToInvite'), disabled: true }],
  };
  const relationship: MenuEntry[] = [
    ...(isFriend
      ? [
          {
            label: t('lounge.menus.removeFriend'),
            danger: true,
            onSelect: () =>
              openDialog({
                kind: 'confirmRelationship',
                action: 'removeFriend',
                userId: user.id,
                name: user.name,
              }),
          } as MenuEntry,
        ]
      : []),
    {
      label: t('lounge.menus.ignore'),
      onSelect: () => after(ignoreUser(user.id), 'lounge.friends.ignoreError'),
    },
    {
      label: t('lounge.menus.block'),
      danger: true,
      onSelect: () =>
        openDialog({
          kind: 'confirmRelationship',
          action: 'block',
          userId: user.id,
          name: user.name,
        }),
    },
    ...entryList(reportEntry({ kind: 'user', id: user.id, authorId: user.id, guildId })),
  ];
  return { note, invite, relationship };
}

import { Avatar, Button } from '@sideroom/ui';
import { useState } from 'react';
import { useT } from '../../i18n';
import { useDms, useGuilds, useUsers } from '../data';
import { useProfile } from '../profile/useProfile';
import { blockUser, removeRelationship } from './api';
import { friendsOf, useSocial } from './social';

/**
 * The top of a one-to-one DM (LNG-DM-005): who this is, the servers you share, and Remove Friend / Block. Group DMs
 * keep the plain welcome text.
 */
export function DmWelcome({ channelId, title }: { channelId: string; title: string }) {
  const t = useT();
  const dm = useDms().find((d) => d.id === channelId);
  const users = useUsers();
  const guilds = useGuilds();
  const rows = useSocial((s) => s.relationships);
  const user = dm && dm.recipientIds.length === 1 ? users[dm.recipientIds[0]!] : undefined;
  const state = useProfile(user?.id ?? '');
  const [error, setError] = useState(false);
  if (!user)
    return (
      <div className="lng-welcome">
        <h2>{title}</h2>
        <p>{t('lounge.messageList.dmStart', { title })}</p>
      </div>
    );
  const mutual = state.kind === 'ready' ? (state.profile.mutual_guilds ?? []) : [];
  const names = mutual.map((g) => guilds.find((x) => x.id === g.id)?.name).filter(Boolean);
  const isFriend = friendsOf(rows).some((r) => r.id === user.id);
  const blocked = rows.some((r) => r.id === user.id && r.type === 2);
  const run = (p: Promise<unknown>) => {
    setError(false);
    // The gateway (RELATIONSHIP_*) refreshes the list; only a failure needs to be shown.
    p.catch(() => setError(true));
  };
  return (
    <div className="lng-welcome lng-dm-welcome">
      <Avatar name={user.displayName} src={user.avatarUrl ?? null} size={80} />
      <h2>{user.displayName}</h2>
      <p className="lng-dm-welcome-handle">{user.username}</p>
      <p>{t('lounge.messageList.dmStart', { title: `@${user.username}` })}</p>
      {names.length > 0 && (
        <p className="lng-muted">
          {t('lounge.dmWelcome.mutualServers', {
            count: names.length,
            names: names.slice(0, 3).join(', '),
          })}
        </p>
      )}
      <div className="lng-welcome-actions">
        {isFriend && (
          <Button size="sm" variant="secondary" onClick={() => run(removeRelationship(user.id))}>
            {t('lounge.dmWelcome.removeFriend')}
          </Button>
        )}
        {!blocked && (
          <Button size="sm" variant="danger" onClick={() => run(blockUser(user.id))}>
            {t('lounge.dmWelcome.block')}
          </Button>
        )}
      </div>
      {error && (
        <p role="alert" className="lng-failed">
          {t('lounge.dmWelcome.error')}
        </p>
      )}
    </div>
  );
}

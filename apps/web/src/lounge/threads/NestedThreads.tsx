import { Link } from '@tanstack/react-router';
import { Icon } from '@sideroom/ui';
import { useMemo } from 'react';
import { useT } from '../../i18n';
import { useViewAsThreadParent } from '../../settings/server/viewAsMode';
import { useLoungeRoute } from '../nav';
import { threadsOf, useThreads } from './threadStore';

/** Threads the viewer follows (plus the one open now), nested under their channel in the list (LNG-THR-007). */
export function NestedThreads({ guildId, channelId }: { guildId: string; channelId: string }) {
  const t = useT();
  const byId = useThreads((s) => s.byId);
  const { threadId } = useLoungeRoute();
  const parentShown = useViewAsThreadParent(guildId);
  const list = useMemo(
    () =>
      parentShown(channelId)
        ? threadsOf(byId, channelId).filter((th) => th.joined || th.id === threadId)
        : [],
    [byId, channelId, threadId, parentShown],
  );
  if (list.length === 0) return null;
  return (
    <ul className="lng-thread-nest" aria-label={t('lounge.nestedThreads.label')}>
      {list.map((th) => (
        <li key={th.id}>
          <Link
            to="/channels/$guildId/$channelId/threads/$threadId"
            params={{ guildId, channelId, threadId: th.id }}
            className="lng-channel lng-thread-link"
            aria-current={th.id === threadId ? 'page' : undefined}
            data-selected={th.id === threadId || undefined}
            draggable={false}
          >
            <Icon name={th.isPrivate ? 'lock' : 'list'} size={16} />
            <span className="lng-channel-name">{th.name}</span>
          </Link>
        </li>
      ))}
    </ul>
  );
}

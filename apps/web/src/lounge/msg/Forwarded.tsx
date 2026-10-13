import type { Resolvers } from '@sideroom/markdown-react';
import { Icon } from '@sideroom/ui';
import type { ForwardSnapshot } from '../data';
import { useAllChannels } from '../data';
import { useLoungeGotoMessage } from '../nav';
import { formatMessageTime } from '../time';
import { AttachmentList } from './Attachments';
import { EmbedList } from './Embeds';
import { Md } from './Md';

/** Frame around a forwarded message's snapshot (LNG-MSGUI-006). */
export function ForwardedFrame({
  snapshot,
  resolvers,
}: {
  snapshot: ForwardSnapshot;
  resolvers?: Resolvers;
}) {
  const channelsByGuild = useAllChannels();
  const goto = useLoungeGotoMessage();
  const src = snapshot.source;
  // The source is named only when the viewer can see its channel; otherwise the label stays plain.
  const known = src
    ? Object.entries(channelsByGuild)
        .flatMap(([gid, list]) => list.map((c) => ({ gid, c })))
        .find((x) => x.c.id === src.channelId)
    : undefined;
  return (
    <div className="lng-forward" role="group" aria-label="Forwarded message">
      <div className="lng-forward-label">
        <Icon name="arrow-up-right" size={16} /> Forwarded
        {src && known && (
          <>
            {' '}
            from{' '}
            <button
              type="button"
              className="lng-link-btn"
              onClick={() => goto(known.gid, src.channelId, src.messageId)}
            >
              #{known.c.name}
            </button>
          </>
        )}
      </div>
      <Md text={snapshot.content} resolvers={resolvers} />
      {snapshot.attachments.length > 0 && <AttachmentList attachments={snapshot.attachments} />}
      {snapshot.embeds.length > 0 && (
        <EmbedList embeds={snapshot.embeds} {...(resolvers ? { resolvers } : {})} />
      )}
      <time className="lng-muted lng-forward-time" dateTime={snapshot.createdAt}>
        {formatMessageTime(snapshot.createdAt)}
      </time>
    </div>
  );
}

import { Avatar, Icon } from '@sideroom/ui';
import type { Resolvers } from '@sideroom/markdown-react';
import { useT } from '../../i18n';
import type { Message } from '../data';
import { SingleLine } from '../msg/SingleLine';
import { useHiddenAuthors } from '../shell/hiddenAuthors';
import { formatFullTime, formatRelativeTime, useMinuteTick } from '../time';

/**
 * The thread row under a message (LNG-THR-006, D-689): the first line is the thread's name and message count;
 * a second line shows who wrote the latest message, its text on one line and when. No latest message (a thread
 * nobody wrote in, or one the server could not preview): one line. A blocked or ignored author's text is not
 * shown: the Lounge's "blocked message" wording stands in for it.
 */
export function ThreadChip({
  thread,
  resolvers,
  onOpen,
}: {
  thread: NonNullable<Message['thread']>;
  resolvers?: Resolvers | undefined;
  onOpen: () => void;
}) {
  const t = useT();
  const hidden = useHiddenAuthors();
  const now = useMinuteTick();
  const last = thread.last;
  const name = last ? last.author.displayName || t('lounge.messageRequests.unknown') : '';
  const kind = last
    ? hidden.blocked.has(last.author.id)
      ? 'blocked'
      : hidden.ignored.has(last.author.id)
        ? 'ignored'
        : null
    : null;
  return (
    <button
      type="button"
      className="lng-thread-chip"
      data-has-last={last ? '' : undefined}
      onClick={onOpen}
    >
      <Icon name="list" size={16} />
      <strong>{thread.name}</strong>
      <span className="lng-muted">
        {t('lounge.messageItem.threadMessageCount', { n: thread.messageCount })}
      </span>
      {last && (
        <span className="lng-thread-last" data-thread-last>
          <Avatar
            name={name}
            src={last.author.avatarUrl ?? null}
            size={24}
            style={{ width: 16, height: 16, fontSize: 8, flexShrink: 0 }}
            aria-hidden="true"
          />
          <span className="lng-thread-last-name">{name}</span>
          <span className="lng-thread-last-text">
            {kind ? (
              <span className="lng-muted">
                {t('lounge.messageList.hiddenRun', { count: 1, kind })}
              </span>
            ) : (
              <>
                {last.content && <SingleLine text={last.content} resolvers={resolvers} />}
                {last.content && last.attachments > 0 && ' \u00b7 '}
                {last.attachments > 0 &&
                  t('lounge.preview.attachments', { count: last.attachments })}
              </>
            )}
          </span>
          <time
            className="lng-thread-last-time"
            dateTime={last.createdAt}
            title={formatFullTime(last.createdAt)}
          >
            {formatRelativeTime(last.createdAt, now)}
          </time>
        </span>
      )}
    </button>
  );
}

import type { ReactNode } from 'react';
import { useT } from '../../i18n';
import { useMarkdownReady } from '../markdown';
import { notificationText, storeResolvers } from '../shell/notifyText';
import { useHiddenAuthors } from '../shell/hiddenAuthors';

/**
 * A message's text where it is not shown whole (D-729, D-730): the reply line above a reply, the Inbox's mention
 * list, the delete confirmation. It is plain text by D-716's rule (spoiler hidden, mentions named, one line, at
 * most 140 characters); when the author is blocked or ignored it is the Lounge's "blocked message" wording and
 * nothing of the text. Nothing here ever draws the raw text: until the parser is loaded the line is empty.
 */

/** Why an author's text is hidden: blocked wins over ignored (the thread row says the same). */
export type HiddenKind = 'blocked' | 'ignored';

/** `kind` for `authorId` from the live block and ignore lists; re-renders when either changes. */
export function useHiddenKind(authorId: string | null | undefined): HiddenKind | null {
  const hidden = useHiddenAuthors();
  if (!authorId) return null;
  return hidden.blocked.has(authorId) ? 'blocked' : hidden.ignored.has(authorId) ? 'ignored' : null;
}

/** The plain line of a message, for code outside a component (the DM toast). Empty when it cannot be made plain. */
export function messageLineText(
  content: string,
  guildId: string | null | undefined,
  spoilerWord: string,
): string {
  return notificationText(content, storeResolvers(guildId), spoilerWord);
}

/** The text of a message as one plain line; `fallback` shows when there is nothing to say. */
export function MessageLine({
  content,
  authorId,
  guildId,
  className,
  fallback,
}: {
  content: string;
  authorId?: string | null | undefined;
  guildId?: string | null | undefined;
  className?: string | undefined;
  fallback?: ReactNode;
}) {
  const t = useT();
  useMarkdownReady();
  const kind = useHiddenKind(authorId);
  if (kind) {
    return (
      <span className={className ? `${className} lng-muted` : 'lng-muted'} data-hidden-line={kind}>
        {t('lounge.messageList.hiddenRun', { count: 1, kind })}
      </span>
    );
  }
  const text = messageLineText(content, guildId, t('lounge.preview.spoiler'));
  if (!text) return fallback ? <span className={className}>{fallback}</span> : null;
  return <span className={className}>{text}</span>;
}

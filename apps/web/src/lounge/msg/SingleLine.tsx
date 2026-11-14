import {
  formatTimestamp,
  type MdNode,
  type Resolvers,
  type TimestampFormatOptions,
} from '@sideroom/markdown-react';
import { useT } from '../../i18n';
import { parseMarkdown, useMarkdownReady } from '../markdown';

const BLOCKS: ReadonlySet<string> = new Set(['list', 'quote', 'heading', 'subtext', 'code_block']);

/**
 * What the message renderer shows for a timestamp node (`Invalid Date` when it cannot), except the relative style
 * (`:R`): a one-line text is not redrawn as time passes, so "in 5 minutes" would go stale. It is the short date and
 * time (the `f` style) instead (D-737).
 */
function timeText(
  unix: number,
  style: Extract<MdNode, { type: 'timestamp' }>['style'],
  time: TimestampFormatOptions | undefined,
): string {
  try {
    return formatTimestamp(unix, style === 'relative' ? 'short_date_time' : style, time);
  } catch {
    return 'Invalid Date';
  }
}

/**
 * One line of plain text for a message that is not shown whole: the Message Requests row (D-688) and the thread
 * row under a message (D-689). Mentions and emoji become text (`@name`, `:name:`), a link is its label, code is
 * its text, a spoiler is hidden, a timestamp is the date and time the message renderer shows, line breaks are spaces and a
 * block (list, quote, heading, subtext, code block) stands apart from the text around it (D-722). It never renders markup, a link or an image: whatever
 * the text says, the result is a string React draws as text.
 */
export function singleLineText(
  nodes: readonly MdNode[],
  resolvers: Resolvers | undefined,
  spoiler: string,
  time?: TimestampFormatOptions,
): string {
  const out: string[] = [];
  const walk = (list: readonly MdNode[]) => {
    let after = false;
    for (const n of list) {
      // A block starts apart from what stands before it, and what follows a block starts apart from it.
      const block = BLOCKS.has(n.type);
      if (block || after) out.push(' ');
      after = block;
      switch (n.type) {
        case 'text':
          out.push(n.text);
          break;
        case 'line_break':
          out.push(' ');
          break;
        case 'inline_code':
        case 'code_block':
          out.push(n.code);
          break;
        case 'emoji':
          out.push(n.emoji);
          break;
        case 'custom_emoji':
          out.push(`:${n.name}:`);
          break;
        case 'user_mention':
          out.push(`@${resolvers?.user?.(String(n.id))?.name ?? 'user'}`);
          break;
        case 'role_mention':
          out.push(`@${resolvers?.role?.(String(n.id))?.name ?? 'role'}`);
          break;
        case 'channel_mention':
          out.push(`#${resolvers?.channel?.(String(n.id))?.name ?? 'channel'}`);
          break;
        case 'everyone':
          out.push('@everyone');
          break;
        case 'here':
          out.push('@here');
          break;
        case 'slash_command':
          out.push(`/${n.name}`);
          break;
        case 'spoiler':
          out.push(spoiler);
          break;
        case 'timestamp':
          out.push(timeText(n.unix, n.style, time));
          break;
        case 'bold':
        case 'italic':
        case 'underline':
        case 'strike':
        case 'link':
          walk(n.children);
          break;
        case 'quote':
        case 'heading':
        case 'subtext':
          walk(n.children);
          out.push(' ');
          break;
        case 'list':
          for (const item of n.items) {
            walk(item);
            out.push(' ');
          }
          break;
        default:
          break;
      }
    }
  };
  walk(nodes);
  return out.join('').replace(/\s+/gu, ' ').trim();
}

/** The one-line text of `text`, CSS-truncated by its container (`white-space: nowrap`, ellipsis). */
export function SingleLine({
  text,
  resolvers,
  className,
}: {
  text: string;
  resolvers?: Resolvers | undefined;
  className?: string | undefined;
}) {
  const t = useT();
  useMarkdownReady();
  const line = singleLineText(parseMarkdown(text), resolvers, t('lounge.preview.spoiler'));
  return <span className={className}>{line}</span>;
}

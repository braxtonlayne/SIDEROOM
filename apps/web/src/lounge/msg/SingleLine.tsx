import { type MdNode, type Resolvers } from '@sideroom/markdown-react';
import { useT } from '../../i18n';
import { parseMarkdown, useMarkdownReady } from '../markdown';

/**
 * One line of plain text for a message that is not shown whole: the Message Requests row (D-688) and the thread
 * row under a message (D-689). Mentions and emoji become text (`@name`, `:name:`), a link is its label, code is
 * its text, a spoiler is hidden, line breaks are spaces. It never renders markup, a link or an image: whatever
 * the text says, the result is a string React draws as text.
 */
export function singleLineText(
  nodes: readonly MdNode[],
  resolvers: Resolvers | undefined,
  spoiler: string,
): string {
  const out: string[] = [];
  const walk = (list: readonly MdNode[]) => {
    for (const n of list) {
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

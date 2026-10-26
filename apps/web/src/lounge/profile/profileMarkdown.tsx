// About Me markdown (RDY-PAR-003, LNG-USET-009): the profile subset of `sr-markdown`. The parser is the shared
// WASM one (`lounge/markdown`); this keeps what fits a short biography (bold, italic, underline, strikethrough,
// spoilers, inline code, links, emoji, line breaks) and flattens the rest to plain text: headings, quotes, lists
// and code blocks lose their block layout, and mentions, timestamps and the Lounge-only chips are dropped.
import { Markdown, nodesToText, type MdNode } from '@sideroom/markdown-react';
import { parseMarkdown, useMarkdownReady } from '../markdown';

const BREAK: MdNode = { type: 'line_break' };

function flatten(nodes: readonly MdNode[]): MdNode[] {
  const out: MdNode[] = [];
  const push = (n: MdNode | MdNode[]) => out.push(...(Array.isArray(n) ? n : [n]));
  for (const n of nodes) {
    switch (n.type) {
      case 'text':
      case 'line_break':
      case 'inline_code':
      case 'emoji':
      case 'custom_emoji':
        push(n);
        break;
      case 'bold':
      case 'italic':
      case 'underline':
      case 'strike':
      case 'spoiler':
        push({ ...n, children: flatten(n.children) });
        break;
      case 'link':
        push({ ...n, children: flatten(n.children) });
        break;
      case 'heading':
      case 'quote':
      case 'subtext':
        push(flatten(n.children));
        push(BREAK);
        break;
      case 'list':
        for (const item of n.items) {
          push(flatten(item));
          push(BREAK);
        }
        break;
      case 'code_block':
        push({ type: 'text', text: n.code });
        push(BREAK);
        break;
      default:
        // Mentions, @everyone, timestamps, slash commands, server navigation, Cinema and Library chips.
        break;
    }
  }
  // No trailing breaks left behind by block nodes.
  while (out.length && out[out.length - 1]!.type === 'line_break') out.pop();
  return out;
}

/** The nodes of `text` that About Me renders. */
export function profileSubset(nodes: readonly MdNode[]): MdNode[] {
  return flatten(nodes);
}

/** Plain text of an About Me (for the accessible name and the search index). */
export const profileBioText = (text: string): string =>
  nodesToText(profileSubset(parseMarkdown(text)));

/**
 * About Me as shown on every profile surface (popout, full profile, the Settings preview), so that all of them
 * render the same text the same way.
 */
export function ProfileBio({ text }: { text: string }) {
  useMarkdownReady();
  if (!text) return null;
  return (
    <div className="lng-pop-bio" data-profile-bio>
      <Markdown nodes={profileSubset(parseMarkdown(text))} />
    </div>
  );
}

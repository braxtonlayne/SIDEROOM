// Server Protection "hide suspicious links" (G-95, D-224). A masked link `[text](url)` whose text names another
// host than its target would hide where a click goes. In a server with the setting on it is shown as its raw
// target followed by a warning. Nothing is scanned or removed: the link still works and the person sees the
// real address first.
import type { MdNode } from '@sideroom/markdown-react';

/** A host-like token in link text: `example.com`, `www.example.org/path`, `https://example.com`. */
const HOST_IN_TEXT = /(?:https?:\/\/)?((?:[a-z0-9-]+\.)+[a-z]{2,})(?::\d+)?(?:[/?#]\S*)?/i;

const plain = (nodes: MdNode[]): string =>
  nodes
    .map((n) => {
      if (n.type === 'text') return n.text;
      if ('children' in n) return plain(n.children);
      return '';
    })
    .join('');

const hostOf = (url: string): string | null => {
  try {
    return new URL(url).hostname.toLowerCase().replace(/^www\./, '');
  } catch {
    return null;
  }
};

/** The host the link text claims when it differs from the real target; null when the link is fine. */
export function suspiciousTarget(node: Extract<MdNode, { type: 'link' }>): string | null {
  if (node.kind !== 'masked') return null;
  const real = hostOf(node.url);
  const claimed = HOST_IN_TEXT.exec(plain(node.children))?.[1]?.toLowerCase().replace(/^www\./, '');
  if (!real || !claimed || claimed === real) return null;
  return real;
}

/**
 * Replaces each suspicious masked link by an auto link showing its raw target, followed by `warning(host)`.
 * Returns the same array when nothing changed.
 */
export function showRawSuspiciousLinks(
  nodes: MdNode[],
  warning: (host: string) => string,
): MdNode[] {
  let changed = false;
  const walk = (list: MdNode[]): MdNode[] =>
    list.flatMap((n): MdNode[] => {
      if (n.type === 'link') {
        const host = suspiciousTarget(n);
        if (host === null) return [n];
        changed = true;
        return [
          { type: 'link', url: n.url, kind: 'auto', children: [{ type: 'text', text: n.url }] },
          { type: 'text', text: ` ${warning(host)}` },
        ];
      }
      if (
        'children' in n &&
        (n.type === 'bold' ||
          n.type === 'italic' ||
          n.type === 'underline' ||
          n.type === 'strike' ||
          n.type === 'spoiler' ||
          n.type === 'quote' ||
          n.type === 'heading' ||
          n.type === 'subtext')
      ) {
        const kids = walk(n.children);
        return [kids === n.children ? n : { ...n, children: kids }];
      }
      return [n];
    });
  const out = walk(nodes);
  return changed ? out : nodes;
}

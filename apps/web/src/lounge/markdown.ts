/**
 * Markdown parsing seam for the Lounge.
 *
 * The source of truth is the Rust `sr-markdown` crate compiled to WASM (`@sideroom/wasm`). Until the WASM
 * bundle is wired into the web build, `parseMarkdown` uses a small TypeScript approximation that covers the
 * common Discord syntax so the shell is usable. `setMarkdownParser` lets the app swap in the WASM parser
 * (`(text) => markdown.parse(text) as MdNode[]`) at startup with no component changes.
 */
import type { MdNode } from '@sideroom/markdown-react';

export type MarkdownParser = (text: string) => MdNode[];

let active: MarkdownParser | null = null;
export function setMarkdownParser(p: MarkdownParser | null) {
  active = p;
  cache.clear();
}

const cache = new Map<string, MdNode[]>();

export function parseMarkdown(text: string): MdNode[] {
  const hit = cache.get(text);
  if (hit) return hit;
  const nodes = (active ?? fallbackParse)(text);
  if (cache.size > 2000) cache.clear();
  cache.set(text, nodes);
  return nodes;
}

/** True when the message is only 1-27 emoji (rendered jumbo). Approximation of the Rust `is_jumbo`. */
export function isJumbo(nodes: readonly MdNode[]): boolean {
  const meaningful = nodes.filter((n) => !(n.type === 'text' && n.text.trim() === ''));
  return (
    meaningful.length > 0 &&
    meaningful.length <= 27 &&
    meaningful.every((n) => n.type === 'emoji' || n.type === 'custom_emoji')
  );
}

/* ---------------------------------------------------------------- fallback */

function fallbackParse(text: string): MdNode[] {
  const out: MdNode[] = [];
  const lines = text.split('\n');
  let i = 0;
  let para: string[] = [];
  const flush = () => {
    if (para.length) {
      pushInline(out, para.join('\n'));
      para = [];
    }
  };
  while (i < lines.length) {
    const line = lines[i]!;
    const fence = /^```(\w*)\s*$/.exec(line);
    if (fence) {
      const body: string[] = [];
      let j = i + 1;
      while (j < lines.length && !/^```\s*$/.test(lines[j]!)) body.push(lines[j++]!);
      if (j < lines.length) {
        flush();
        out.push({ type: 'code_block', lang: fence[1] || null, code: body.join('\n') });
        i = j + 1;
        continue;
      }
    }
    const quote = /^>\s?(.*)$/.exec(line);
    if (quote) {
      flush();
      const body = [quote[1]!];
      let j = i + 1;
      while (j < lines.length && /^>\s?/.test(lines[j]!))
        body.push(lines[j++]!.replace(/^>\s?/, ''));
      const children: MdNode[] = [];
      pushInline(children, body.join('\n'));
      out.push({ type: 'quote', children });
      i = j;
      continue;
    }
    const heading = /^(#{1,3})\s+(.+)$/.exec(line);
    if (heading) {
      flush();
      const children: MdNode[] = [];
      pushInline(children, heading[2]!);
      out.push({ type: 'heading', level: heading[1]!.length, children });
      i++;
      continue;
    }
    para.push(line);
    i++;
  }
  flush();
  return out;
}

interface Rule {
  re: RegExp;
  make: (m: RegExpExecArray) => MdNode | null;
}

const wrap = (
  type: 'bold' | 'italic' | 'underline' | 'strike' | 'spoiler',
  inner: string,
): MdNode => {
  const children: MdNode[] = [];
  pushInline(children, inner);
  return { type, children };
};

const RULES: Rule[] = [
  { re: /^`([^`\n]+)`/, make: (m) => ({ type: 'inline_code', code: m[1]! }) },
  { re: /^<@!?(\d+)>/, make: (m) => ({ type: 'user_mention', id: m[1]! }) },
  { re: /^<@&(\d+)>/, make: (m) => ({ type: 'role_mention', id: m[1]! }) },
  { re: /^<#(\d+)>/, make: (m) => ({ type: 'channel_mention', id: m[1]! }) },
  { re: /^@everyone/, make: () => ({ type: 'everyone' }) },
  { re: /^@here/, make: () => ({ type: 'here' }) },
  {
    re: /^<t:(-?\d+)(?::([tTdDfFR]))?>/,
    make: (m) => ({ type: 'timestamp', unix: Number(m[1]), style: null }),
  },
  { re: /^\|\|([\s\S]+?)\|\|/, make: (m) => wrap('spoiler', m[1]!) },
  { re: /^\*\*([\s\S]+?)\*\*/, make: (m) => wrap('bold', m[1]!) },
  { re: /^__([\s\S]+?)__/, make: (m) => wrap('underline', m[1]!) },
  { re: /^~~([\s\S]+?)~~/, make: (m) => wrap('strike', m[1]!) },
  { re: /^\*([^*\s][^*]*?)\*/, make: (m) => wrap('italic', m[1]!) },
  { re: /^_([^_\s][^_]*?)_(?![\w])/, make: (m) => wrap('italic', m[1]!) },
  {
    re: /^\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/,
    make: (m) => ({
      type: 'link',
      url: m[2]!,
      kind: 'masked',
      children: [{ type: 'text', text: m[1]! }],
    }),
  },
  {
    re: /^https?:\/\/[^\s<]+/,
    make: (m) => ({
      type: 'link',
      url: m[0],
      kind: 'auto',
      children: [{ type: 'text', text: m[0] }],
    }),
  },
];

function pushText(out: MdNode[], s: string) {
  if (!s) return;
  const parts = s.split('\n');
  parts.forEach((p, i) => {
    if (i > 0) out.push({ type: 'line_break' });
    if (p) {
      const last = out[out.length - 1];
      if (last?.type === 'text') last.text += p;
      else out.push({ type: 'text', text: p });
    }
  });
}

function pushInline(out: MdNode[], src: string) {
  let rest = src;
  let buf = '';
  while (rest.length) {
    let matched = false;
    // Only try rules at positions that could start one (cheap guard).
    if (/^[`<@|*_~[h]/.test(rest)) {
      for (const rule of RULES) {
        const m = rule.re.exec(rest);
        if (!m) continue;
        const node = rule.make(m);
        if (!node) continue;
        pushText(out, buf);
        buf = '';
        out.push(node);
        rest = rest.slice(m[0].length);
        matched = true;
        break;
      }
    }
    if (!matched) {
      buf += rest[0];
      rest = rest.slice(1);
    }
  }
  pushText(out, buf);
}

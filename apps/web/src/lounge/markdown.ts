/**
 * Markdown seam for the Lounge.
 *
 * The parser is the Rust `sr-markdown` crate compiled to WASM, loaded lazily through `@sideroom/wasm`.
 * `loadMarkdownEngine()` starts that load (the Lounge shell calls it on mount). Until it resolves, messages
 * render as plain text so nothing waits on the download. Unit tests inject `testFallbackEngine`, a small TS
 * stand-in, through `setMarkdownEngine` instead of loading the bundle.
 */
import type { MarkdownApi } from '@sideroom/wasm';
import type { MdNode } from '@sideroom/markdown-react';
import { useSyncExternalStore } from 'react';

export interface MarkdownEngine {
  parse(text: string): MdNode[];
  /** True for 1 to 27 emoji and nothing else (rendered jumbo). Same rule as the server. */
  isJumbo(text: string): boolean;
  /** Replaces `:shortcode:` names with unicode emoji. The composer runs this at send time. */
  expandShortcodes(text: string): string;
}

let engine: MarkdownEngine | null = null;
let loading: Promise<MarkdownEngine> | null = null;
const listeners = new Set<() => void>();
const cache = new Map<string, MdNode[]>();

function publish(next: MarkdownEngine | null) {
  engine = next;
  cache.clear();
  for (const listener of listeners) listener();
}

/** Swaps the active engine; `null` returns to plain-text rendering. Tests use this to inject stand-ins. */
export function setMarkdownEngine(next: MarkdownEngine | null): void {
  publish(next);
}

/** The engine once loaded, otherwise `null`. */
export function currentMarkdownEngine(): MarkdownEngine | null {
  return engine;
}

async function loadWasmEngine(): Promise<MarkdownEngine> {
  const { loadMarkdown } = await import('@sideroom/wasm');
  return fromWasm(await loadMarkdown());
}

function fromWasm(md: MarkdownApi): MarkdownEngine {
  return {
    parse: (text) => md.parse(text) as unknown as MdNode[],
    isJumbo: (text) => md.isJumbo(text),
    expandShortcodes: (text) => md.expandShortcodes(text),
  };
}

/** Loads the WASM parser once. Resolves at once when an engine is already active. */
export function loadMarkdownEngine(): Promise<MarkdownEngine> {
  if (engine) return Promise.resolve(engine);
  loading ??= loadWasmEngine().then(
    (loaded) => {
      publish(loaded);
      return loaded;
    },
    (err: unknown) => {
      loading = null;
      throw err;
    },
  );
  return loading;
}

const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};
const isReady = () => engine !== null;

/** Re-renders the caller once the WASM parser is ready. */
export function useMarkdownReady(): boolean {
  return useSyncExternalStore(subscribe, isReady, isReady);
}

export function parseMarkdown(text: string): MdNode[] {
  if (!engine) return plainNodes(text);
  const hit = cache.get(text);
  if (hit) return hit;
  const nodes = engine.parse(text);
  if (cache.size > 2000) cache.clear();
  cache.set(text, nodes);
  return nodes;
}

/** False until the engine is ready; the caller re-renders through `useMarkdownReady`. */
export function isJumbo(text: string): boolean {
  return engine?.isJumbo(text) ?? false;
}

function plainNodes(text: string): MdNode[] {
  const out: MdNode[] = [];
  pushText(out, text);
  return out;
}

/* ------------------------------------------------- test-only stand-in */

/**
 * A small TypeScript approximation of the common Discord syntax, for unit tests only. It does not
 * expand shortcodes and it never reports jumbo for text; production always uses the WASM engine.
 */
export const testFallbackEngine: MarkdownEngine = {
  parse: (text) => fallbackParse(text),
  isJumbo: (text) => {
    const meaningful = fallbackParse(text).filter(
      (n) => !(n.type === 'text' && n.text.trim() === ''),
    );
    return (
      meaningful.length > 0 &&
      meaningful.length <= 27 &&
      meaningful.every((n) => n.type === 'custom_emoji')
    );
  },
  expandShortcodes: (text) => text,
};

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

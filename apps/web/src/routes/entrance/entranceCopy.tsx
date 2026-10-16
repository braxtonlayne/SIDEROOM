// Operator-edited Entrance copy (ENT-UI-010/025): `GET /sr/entrance/copy` answers `{key: markdown}` for the blocks
// staff overrode; a missing key means the built-in default (the catalog text) stays. This file is part of the
// first-paint core: it has no t() keys and imports nothing heavy, and the markdown is rendered by the small
// restricted renderer below (paragraphs, headings, lists, bold, italic, code, http(s) links; never raw HTML).
import { useEffect, useState, type ReactNode } from 'react';
import { apiFetch } from '../../api/client';

export type EntranceCopyKey = 'no-invitation' | 'house-notes' | 'landing-note' | 'hero-alt';
export type EntranceCopy = Partial<Record<EntranceCopyKey, string>>;

let cache: EntranceCopy | null = null;
let inflight: Promise<EntranceCopy> | null = null;
const listeners = new Set<(c: EntranceCopy) => void>();

export function fetchEntranceCopy(): Promise<EntranceCopy> {
  return apiFetch<EntranceCopy>('/sr/entrance/copy', { retries: 0 });
}

function load(): Promise<EntranceCopy> {
  inflight ??= fetchEntranceCopy()
    .then((c) => {
      cache = c && typeof c === 'object' ? c : {};
      return cache;
    })
    .catch(() => {
      // Offline or an old server: keep the built-in copy, and try again on the next page.
      return {} as EntranceCopy;
    })
    .finally(() => {
      inflight = null;
    });
  return inflight;
}

/** Drops the cached copy (the Admin editor calls this after a save) and tells mounted pages. */
export function setEntranceCopy(next: EntranceCopy | null) {
  cache = next;
  if (next) listeners.forEach((l) => l(next));
}

/** The operator copy, `{}` until it arrives or when the server has none. */
export function useEntranceCopy(): EntranceCopy {
  const [copy, setCopy] = useState<EntranceCopy>(cache ?? {});
  useEffect(() => {
    let live = true;
    listeners.add(setCopy);
    if (cache) setCopy(cache);
    else
      void load().then((c) => {
        if (live) setCopy(c);
      });
    return () => {
      live = false;
      listeners.delete(setCopy);
    };
  }, []);
  return copy;
}

// ---- restricted markdown ----

const INLINE = /(`[^`]+`|\*\*[^*]+\*\*|\*[^*\s][^*]*\*|\[[^\]]+\]\([^)\s]+\))/g;

function safeHref(href: string): string | null {
  if (/^https?:\/\//i.test(href)) return href;
  if (href.startsWith('/') && !href.startsWith('//')) return href;
  return null;
}

function inline(text: string, keyBase: string): ReactNode[] {
  return text.split(INLINE).map((part, i) => {
    const key = `${keyBase}-${i}`;
    if (part.startsWith('`') && part.endsWith('`') && part.length > 2)
      return <code key={key}>{part.slice(1, -1)}</code>;
    if (part.startsWith('**') && part.endsWith('**') && part.length > 4)
      return <strong key={key}>{inline(part.slice(2, -2), key)}</strong>;
    if (part.startsWith('*') && part.endsWith('*') && part.length > 2)
      return <em key={key}>{inline(part.slice(1, -1), key)}</em>;
    const link = /^\[([^\]]+)\]\(([^)\s]+)\)$/.exec(part);
    if (link) {
      const href = safeHref(link[2] ?? '');
      if (href)
        return (
          <a key={key} href={href} rel="noopener noreferrer nofollow">
            {link[1]}
          </a>
        );
      return link[1];
    }
    return part;
  });
}

/** Renders operator markdown as React elements. Headings start at h2 (the page keeps its own h1). */
export function EntranceMarkdown({ source }: { source: string }) {
  const lines = source.replace(/\r\n?/g, '\n').split('\n');
  const out: ReactNode[] = [];
  let para: string[] = [];
  let list: { ordered: boolean; items: string[] } | null = null;
  let n = 0;
  const flushPara = () => {
    if (para.length) {
      const key = `p${n++}`;
      out.push(<p key={key}>{inline(para.join(' '), key)}</p>);
      para = [];
    }
  };
  const flushList = () => {
    if (list) {
      const key = `l${n++}`;
      const items = list.items.map((it, i) => <li key={i}>{inline(it, `${key}-${i}`)}</li>);
      out.push(list.ordered ? <ol key={key}>{items}</ol> : <ul key={key}>{items}</ul>);
      list = null;
    }
  };
  for (const raw of lines) {
    const line = raw.trimEnd();
    const heading = /^(#{1,4})\s+(.*)$/.exec(line);
    const bullet = /^\s*[-*]\s+(.*)$/.exec(line);
    const numbered = /^\s*\d+[.)]\s+(.*)$/.exec(line);
    if (!line.trim()) {
      flushPara();
      flushList();
    } else if (heading) {
      flushPara();
      flushList();
      const level = Math.min(4, Math.max(2, (heading[1] ?? '#').length));
      const key = `h${n++}`;
      const Tag = `h${level}` as 'h2' | 'h3' | 'h4';
      // An id from the text keeps footer anchors such as /house-notes#privacy working for operator copy.
      const id = (heading[2] ?? '')
        .toLowerCase()
        .replace(/[^\p{L}\p{N}]+/gu, '-')
        .replace(/^-+|-+$/g, '');
      out.push(
        <Tag key={key} {...(id ? { id } : {})}>
          {inline(heading[2] ?? '', key)}
        </Tag>,
      );
    } else if (bullet || numbered) {
      flushPara();
      const ordered = !bullet;
      if (list && list.ordered !== ordered) flushList();
      list ??= { ordered, items: [] };
      list.items.push((bullet ?? numbered)?.[1] ?? '');
    } else {
      flushList();
      para.push(line.trim());
    }
  }
  flushPara();
  flushList();
  return <div className="ent-copy">{out}</div>;
}

// ---- operator hero image (H-41, D-239) ----

const HERO_URL = '/api/v1/sr/entrance/hero';
let heroCache: boolean | null = null;

/** Forget what the page knows about the hero image (the Admin editor calls this after a change). */
export function resetEntranceHero() {
  heroCache = null;
}

/**
 * The operator's hero image URL when the server has one (`GET /sr/entrance/hero` answers 404
 * otherwise), else null and the shipped art stays. The browser caches the image by ETag, so
 * the `<img>` that follows costs no second download.
 */
export function useEntranceHero(): string | null {
  const [has, setHas] = useState<boolean | null>(heroCache);
  useEffect(() => {
    if (heroCache !== null) return;
    let live = true;
    void fetch(HERO_URL, { credentials: 'omit' })
      .then((res) => {
        void res.body?.cancel().catch(() => undefined);
        heroCache = res.ok;
        if (live) setHas(res.ok);
      })
      .catch(() => {
        // Offline or an old server: the shipped art stays; ask again on the next page.
      });
    return () => {
      live = false;
    };
  }, []);
  return has ? HERO_URL : null;
}

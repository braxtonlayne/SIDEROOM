// Operator-edited Entrance copy (ENT-UI-010/025): `GET /sr/entrance/copy` answers `{key: markdown}` for the blocks
// staff overrode; a missing key means the built-in default (the catalog text) stays. This file is part of the
// first-paint core: it has no t() keys and imports nothing heavy, and the markdown is rendered by the small
// restricted renderer below (paragraphs, headings, lists, code blocks, quotes, tables, bold, italic, code,
// http(s), same-site and same-page links; never raw HTML).
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

const INLINE =
  /(`[^`]+`|\*\*[^*]+\*\*|\*[^*\s][^*]*\*|(?<![\p{L}\p{N}_])_[^_\s][^_]*_(?![\p{L}\p{N}_])|\[[^\]]+\]\([^)\s]+\))/gu;

function safeHref(href: string): string | null {
  if (/^https?:\/\//i.test(href)) return href;
  if (href.startsWith('/') && !href.startsWith('//')) return href;
  // An anchor on the same page (the in-app runbook links its own playbooks).
  if (/^#[\p{L}\p{N}_-]+$/u.test(href)) return href;
  return null;
}

function inline(text: string, keyBase: string): ReactNode[] {
  return text.split(INLINE).map((part, i) => {
    const key = `${keyBase}-${i}`;
    if (part.startsWith('`') && part.endsWith('`') && part.length > 2)
      return <code key={key}>{part.slice(1, -1)}</code>;
    if (part.startsWith('**') && part.endsWith('**') && part.length > 4)
      return <strong key={key}>{inline(part.slice(2, -2), key)}</strong>;
    if (
      ((part.startsWith('*') && part.endsWith('*')) ||
        (part.startsWith('_') && part.endsWith('_'))) &&
      part.length > 2
    )
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

/** The default heading id: lower case, runs of other characters become one dash. */
function defaultHeadingId(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, '-')
    .replace(/^-+|-+$/g, '');
}

const cells = (row: string) =>
  row
    .trim()
    .replace(/^\|/, '')
    .replace(/\|$/, '')
    .split('|')
    .map((c) => c.trim());

/**
 * Renders operator markdown as React elements. Headings start at h2 (the page keeps its own h1). Supported:
 * paragraphs, headings, lists (an ordered list keeps its first number), fenced code, block quotes, simple
 * tables and the inline marks above. `headingId` replaces the id rule (the runbook uses the playbook slug).
 */
export function EntranceMarkdown({
  source,
  headingId = defaultHeadingId,
}: {
  source: string;
  headingId?: (text: string) => string;
}) {
  const lines = source.replace(/\r\n?/g, '\n').split('\n');
  const out: ReactNode[] = [];
  let para: string[] = [];
  let quote: string[] = [];
  let table: string[] = [];
  let list: { ordered: boolean; start: number; items: string[] } | null = null;
  let fence: { indent: number; lines: string[] } | null = null;
  let n = 0;
  const flushPara = () => {
    if (para.length) {
      const key = `p${n++}`;
      out.push(<p key={key}>{inline(para.join(' '), key)}</p>);
      para = [];
    }
  };
  const flushQuote = () => {
    if (quote.length) {
      const key = `q${n++}`;
      out.push(
        <blockquote key={key}>
          <p>{inline(quote.join(' '), key)}</p>
        </blockquote>,
      );
      quote = [];
    }
  };
  const flushTable = () => {
    if (table.length) {
      const key = `t${n++}`;
      const rows = table.filter((r) => !/^\s*\|?[\s:|-]+\|?\s*$/.test(r));
      const [head, ...body] = rows;
      out.push(
        <table key={key}>
          {head && (
            <thead>
              <tr>
                {cells(head).map((c, i) => (
                  <th key={i} scope="col">
                    {inline(c, `${key}-h${i}`)}
                  </th>
                ))}
              </tr>
            </thead>
          )}
          <tbody>
            {body.map((r, ri) => (
              <tr key={ri}>
                {cells(r).map((c, i) => (
                  <td key={i}>{inline(c, `${key}-${ri}-${i}`)}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>,
      );
      table = [];
    }
  };
  const flushList = () => {
    if (list) {
      const key = `l${n++}`;
      const items = list.items.map((it, i) => <li key={i}>{inline(it, `${key}-${i}`)}</li>);
      out.push(
        list.ordered ? (
          <ol key={key} {...(list.start !== 1 ? { start: list.start } : {})}>
            {items}
          </ol>
        ) : (
          <ul key={key}>{items}</ul>
        ),
      );
      list = null;
    }
  };
  const flushAll = () => {
    flushPara();
    flushQuote();
    flushTable();
    flushList();
  };
  for (const raw of lines) {
    const line = raw.trimEnd();
    const fenceMark = /^(\s*)```/.exec(line);
    if (fence) {
      if (fenceMark) {
        const key = `c${n++}`;
        out.push(
          <pre key={key}>
            <code>{fence.lines.join('\n')}</code>
          </pre>,
        );
        fence = null;
      } else {
        const indent = fence.indent;
        fence.lines.push(line.slice(Math.min(indent, line.length - line.trimStart().length)));
      }
      continue;
    }
    if (fenceMark) {
      // A fence inside a list item closes the list; the next item keeps its own number (`start`).
      flushAll();
      fence = { indent: fenceMark[1]?.length ?? 0, lines: [] };
      continue;
    }
    const heading = /^(#{1,4})\s+(.*)$/.exec(line);
    const bullet = /^\s*[-*]\s+(.*)$/.exec(line);
    const numbered = /^\s*(\d+)[.)]\s+(.*)$/.exec(line);
    const quoted = /^>\s?(.*)$/.exec(line);
    if (!line.trim()) {
      flushPara();
      flushQuote();
      flushTable();
    } else if (heading) {
      flushAll();
      const level = Math.min(4, Math.max(2, (heading[1] ?? '#').length));
      const key = `h${n++}`;
      const Tag = `h${level}` as 'h2' | 'h3' | 'h4';
      // An id from the text keeps footer anchors such as /house-notes#privacy working for operator copy.
      const id = headingId(heading[2] ?? '');
      out.push(
        <Tag key={key} {...(id ? { id } : {})}>
          {inline(heading[2] ?? '', key)}
        </Tag>,
      );
    } else if (quoted) {
      flushPara();
      flushTable();
      flushList();
      quote.push((quoted[1] ?? '').trim());
    } else if (line.trimStart().startsWith('|')) {
      flushPara();
      flushQuote();
      flushList();
      table.push(line);
    } else if (bullet || numbered) {
      flushPara();
      flushQuote();
      flushTable();
      const ordered = !bullet;
      if (list && list.ordered !== ordered) flushList();
      const number = numbered ? Number(numbered[1]) : 1;
      list ??= { ordered, start: number, items: [] };
      list.items.push((bullet ? bullet[1] : numbered?.[2]) ?? '');
    } else if (list && /^\s+\S/.test(raw)) {
      // An indented paragraph under a list item: shown after the list; the next item keeps its own number.
      flushList();
      para.push(line.trim());
    } else {
      flushList();
      flushQuote();
      flushTable();
      para.push(line.trim());
    }
  }
  flushAll();
  const open = fence as { lines: string[] } | null;
  if (open)
    out.push(
      <pre key={`c${n++}`}>
        <code>{open.lines.join('\n')}</code>
      </pre>,
    );
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

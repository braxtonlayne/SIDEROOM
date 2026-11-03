/**
 * The Vite dev-server mirror of `crates/sr-server/src/house_notes_md.rs` (FU-22): restricted markdown for the
 * `house-notes` Entrance copy as static HTML. Same subset, same output byte for byte; the fixtures in
 * `crates/sr-server/tests/fixtures/house-notes` are read by both tests. Written as a scanner (no regex alternation)
 * so the two implementations follow the same steps. Every text node is escaped; raw HTML, images and autolinks
 * stay text.
 */

export function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

const isWs = (c: string) => /\s/u.test(c);
const isWord = (c: string) => /[\p{L}\p{N}_]/u.test(c);

function first(c: string[], from: number, want: string): number {
  for (let k = from; k < c.length; k++) if (c[k] === want) return k;
  return -1;
}

const span = (c: string[], a: number, b: number) => c.slice(a, b).join('');

function safeHref(href: string): boolean {
  const lower = href.toLowerCase();
  if (lower.startsWith('https://') || lower.startsWith('http://')) return true;
  if (href.startsWith('/')) return !href.startsWith('//');
  return /^#[\p{L}\p{N}_-]+$/u.test(href);
}

function construct(c: string[], i: number): [string, number] | null {
  const ch = c[i];
  if (ch === '`') {
    const k = first(c, i + 1, '`');
    if (k <= i + 1) return null;
    return [`<code>${escapeHtml(span(c, i + 1, k))}</code>`, k + 1];
  }
  if (ch === '*' && c[i + 1] === '*') {
    const k = first(c, i + 2, '*');
    if (k <= i + 2 || c[k + 1] !== '*') return null;
    return [`<strong>${inline(span(c, i + 2, k))}</strong>`, k + 2];
  }
  if (ch === '*') {
    const n = c[i + 1];
    if (n === undefined || n === '*' || isWs(n)) return null;
    const k = first(c, i + 2, '*');
    if (k < 0) return null;
    return [`<em>${inline(span(c, i + 1, k))}</em>`, k + 1];
  }
  if (ch === '_') {
    const p = i > 0 ? c[i - 1] : undefined;
    if (p !== undefined && isWord(p)) return null;
    const n = c[i + 1];
    if (n === undefined || n === '_' || isWs(n)) return null;
    const k = first(c, i + 2, '_');
    if (k < 0) return null;
    const a = c[k + 1];
    if (a !== undefined && isWord(a)) return null;
    return [`<em>${inline(span(c, i + 1, k))}</em>`, k + 1];
  }
  if (ch === '[') {
    if (i > 0 && c[i - 1] === '!') return null; // an image stays text
    const k = first(c, i + 1, ']');
    if (k <= i + 1 || c[k + 1] !== '(') return null;
    let m = k + 2;
    while (m < c.length && c[m] !== ')' && !isWs(c[m] ?? '')) m++;
    if (m <= k + 2 || c[m] !== ')') return null;
    const text = escapeHtml(span(c, i + 1, k));
    const href = span(c, k + 2, m);
    return [
      safeHref(href) ? `<a href="${escapeHtml(href)}" rel="noopener noreferrer">${text}</a>` : text,
      m + 1,
    ];
  }
  return null;
}

function inline(text: string): string {
  const c = Array.from(text);
  let out = '';
  let i = 0;
  while (i < c.length) {
    const hit = construct(c, i);
    if (hit) {
      out += hit[0];
      i = hit[1];
    } else {
      out += escapeHtml(c[i] ?? '');
      i += 1;
    }
  }
  return out;
}

function slug(text: string): string {
  let out = '';
  let dash = false;
  for (const ch of text) {
    if (/[A-Za-z0-9]/.test(ch)) {
      if (dash && out) out += '-';
      dash = false;
      out += ch.toLowerCase();
    } else dash = true;
  }
  return out;
}

const trimEnd = (s: string) => s.replace(/\s+$/u, '');
const trimStart = (s: string) => s.replace(/^\s+/u, '');

function headingOf(line: string): [number, string] | null {
  let hashes = 0;
  while (line[hashes] === '#') hashes++;
  if (hashes < 1 || hashes > 4) return null;
  const rest = line.slice(hashes);
  const text = trimStart(rest);
  return text.length < rest.length ? [Math.min(4, hashes + 1), text] : null;
}

function bulletOf(line: string): string | null {
  const t = trimStart(line);
  if (t[0] !== '-' && t[0] !== '*') return null;
  const rest = t.slice(1);
  const text = trimStart(rest);
  return text.length < rest.length && text ? text : null;
}

function numberedOf(line: string): [number, string] | null {
  const t = trimStart(line);
  let d = 0;
  while (t[d] !== undefined && t[d]! >= '0' && t[d]! <= '9') d++;
  if (d === 0 || (t[d] !== '.' && t[d] !== ')')) return null;
  const rest = t.slice(d + 1);
  const text = trimStart(rest);
  return text.length < rest.length && text ? [Number(t.slice(0, d)), text] : null;
}

const isSeparatorRow = (row: string) => row !== '' && /^[\s:|-]+$/u.test(row);

function cells(row: string): string[] {
  let t = row.trim();
  if (t.startsWith('|')) t = t.slice(1);
  if (t.endsWith('|')) t = t.slice(0, -1);
  return t.split('|').map((x) => x.trim());
}

/** Render `copy` as HTML; empty or blank copy gives an empty string. */
export function renderHouseNotes(copy: string): string {
  const lines = copy.replace(/\r\n/g, '\n').replace(/\r/g, '\n').split('\n');
  let out = '';
  let para: string[] = [];
  let quote: string[] = [];
  let table: string[] = [];
  let list: { ordered: boolean; start: number; items: string[] } | null = null;
  let fence: { indent: number; lines: string[] } | null = null;
  const ids: string[] = [];

  const flushPara = () => {
    if (para.length) out += `<p>${para.map(inline).join('<br>')}</p>\n`;
    para = [];
  };
  const flushQuote = () => {
    if (quote.length) out += `<blockquote><p>${inline(quote.join(' '))}</p></blockquote>\n`;
    quote = [];
  };
  const flushTable = () => {
    if (!table.length) return;
    const rows = table.filter((r) => !isSeparatorRow(r));
    table = [];
    out += '<table>\n';
    const [head, ...body] = rows;
    if (head !== undefined) {
      out += `<thead><tr>${cells(head)
        .map((c) => `<th scope="col">${inline(c)}</th>`)
        .join('')}</tr></thead>\n`;
    }
    out += '<tbody>\n';
    for (const r of body)
      out += `<tr>${cells(r)
        .map((c) => `<td>${inline(c)}</td>`)
        .join('')}</tr>\n`;
    out += '</tbody>\n</table>\n';
  };
  const flushList = () => {
    if (!list) return;
    const tag = list.ordered ? 'ol' : 'ul';
    out += list.ordered && list.start !== 1 ? `<ol start="${list.start}">\n` : `<${tag}>\n`;
    for (const it of list.items) out += `<li>${inline(it)}</li>\n`;
    out += `</${tag}>\n`;
    list = null;
  };
  const flushAll = () => {
    flushPara();
    flushQuote();
    flushTable();
    flushList();
  };
  const headingId = (text: string): string | null => {
    const s = slug(text);
    if (!s) return null;
    let id = `op-${s}`;
    let n = 2;
    while (ids.includes(id)) id = `op-${s}-${n++}`;
    ids.push(id);
    return id;
  };

  for (const raw of lines) {
    const line = trimEnd(raw);
    const lead = line.length - trimStart(line).length;
    const fenceMark = trimStart(line).startsWith('```');
    if (fence) {
      if (fenceMark) {
        out += `<pre><code>${escapeHtml(fence.lines.join('\n'))}</code></pre>\n`;
        fence = null;
      } else fence.lines.push(line.slice(Math.min(fence.indent, lead)));
      continue;
    }
    if (fenceMark) {
      flushAll();
      fence = { indent: lead, lines: [] };
      continue;
    }
    const heading = headingOf(line);
    const bullet = bulletOf(line);
    const numbered = numberedOf(line);
    if (!line.trim()) {
      flushPara();
      flushQuote();
      flushTable();
    } else if (heading) {
      flushAll();
      const id = headingId(heading[1]);
      out += `<h${heading[0]}${id ? ` id="${id}"` : ''}>${inline(heading[1])}</h${heading[0]}>\n`;
    } else if (line.startsWith('>')) {
      flushPara();
      flushTable();
      flushList();
      let q = line.slice(1);
      if (q !== '' && isWs(q[0] ?? '')) q = q.slice(1);
      quote.push(q.trim());
    } else if (trimStart(line).startsWith('|')) {
      flushPara();
      flushQuote();
      flushList();
      table.push(line);
    } else if (bullet !== null || numbered !== null) {
      flushPara();
      flushQuote();
      flushTable();
      const ordered = bullet === null;
      if (list && list.ordered !== ordered) flushList();
      list ??= { ordered, start: numbered ? numbered[0] : 1, items: [] };
      list.items.push(bullet ?? numbered?.[1] ?? '');
    } else if (list && lead > 0) {
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
  if (open) out += `<pre><code>${escapeHtml(open.lines.join('\n'))}</code></pre>\n`;
  return out;
}

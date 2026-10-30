/**
 * UTL-SHL-001: the drawer's tool filter. A deliberately small, pure stand-in for the three-stage search of
 * UTL-SRC-001 (`sr-tools-wasm`): this one ranks by name prefix, then alias prefix, then a word inside the name,
 * aliases or description, then a one-letter typo of a name or alias word. The drawer calls only `searchTools`, so
 * the wasm search replaces this function without a change anywhere else.
 */
import type { ToolEntry } from '../registry';

export interface ToolText {
  entry: ToolEntry;
  name: string;
  description: string;
}

const norm = (s: string): string =>
  s
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase()
    .trim();

/** Edit distance, capped: returns `max + 1` as soon as it is clear the words differ by more than `max`. */
export function editDistance(a: string, b: string, max = 1): number {
  if (Math.abs(a.length - b.length) > max) return max + 1;
  let prev = Array.from({ length: b.length + 1 }, (_, i) => i);
  for (let i = 1; i <= a.length; i++) {
    const cur = [i];
    let rowMin = i;
    for (let j = 1; j <= b.length; j++) {
      const v = Math.min(
        (prev[j] ?? 0) + 1,
        (cur[j - 1] ?? 0) + 1,
        (prev[j - 1] ?? 0) + (a[i - 1] === b[j - 1] ? 0 : 1),
      );
      cur.push(v);
      if (v < rowMin) rowMin = v;
    }
    if (rowMin > max) return max + 1;
    prev = cur;
  }
  return prev[b.length] ?? max + 1;
}

/** Entries matching `query`, best first; an empty query returns all of them in registry order. */
export function searchTools(list: readonly ToolText[], query: string): ToolText[] {
  const q = norm(query);
  if (q === '') return [...list];
  const scored: Array<{ item: ToolText; score: number; i: number }> = [];
  list.forEach((item, i) => {
    const name = norm(item.name);
    const aliases = item.entry.aliases.map(norm);
    const words = [name, ...aliases].flatMap((s) => s.split(/[^a-z0-9]+/).filter(Boolean));
    let score = 0;
    if (name === q) score = 100;
    else if (name.startsWith(q)) score = 90;
    else if (aliases.some((a) => a === q || a.startsWith(q))) score = 80;
    else if (words.some((w) => w.startsWith(q))) score = 70;
    else if (name.includes(q) || aliases.some((a) => a.includes(q))) score = 60;
    else if (norm(item.description).includes(q)) score = 40;
    else if (q.length >= 4 && words.some((w) => w.length >= 4 && editDistance(q, w, 1) <= 1))
      score = 30;
    if (score > 0) scored.push({ item, score, i });
  });
  return scored.sort((a, b) => b.score - a.score || a.i - b.i).map((s) => s.item);
}

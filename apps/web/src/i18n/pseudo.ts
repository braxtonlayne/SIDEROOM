import type { Node } from './icu';

const FROM = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ';
const TO = 'àƀçðéƒĝĥîĵķļɱñöþǫŕšţûṽŵẋýžÀƁÇÐÉƑĜĤÎĴĶĻṀÑÖÞǪŔŠŢÛṼŴẊÝŽ';

/** Accent the letters and pad about 35%, readable enough for tests, long enough to expose overflow. */
export function pseudoText(s: string): string {
  const accented = [...s].map((c) => {
    const i = FROM.indexOf(c);
    return i < 0 ? c : [...TO][i]!;
  });
  const pad = '·'.repeat(Math.ceil(s.replace(/\s/g, '').length * 0.35));
  return accented.join('') + pad;
}

/** Pseudo-localise literal text only; arguments, plural structure and `#` are left intact. */
export function pseudoNodes(nodes: Node[]): Node[] {
  return nodes.map((n): Node => {
    if (n.t === 'text') return { t: 'text', v: pseudoText(n.v) };
    if (n.t === 'plural' || n.t === 'select') {
      const cases = new Map<string, Node[]>();
      for (const [k, v] of n.cases) cases.set(k, pseudoNodes(v));
      return { ...n, cases } as Node;
    }
    return n;
  });
}

/** Brackets make truncation at either end visible. */
export function wrapPseudo(nodes: Node[]): Node[] {
  return [{ t: 'text', v: '[' }, ...pseudoNodes(nodes), { t: 'text', v: ']' }];
}

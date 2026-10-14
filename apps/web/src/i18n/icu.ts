/**
 * A small ICU MessageFormat subset (XC-I18N-001), built on `Intl`.
 *
 * Supported: `{name}`, `{n, number}`, `{n, number, percent|integer}`, `{d, date}`, `{d, date, short|medium|long}`,
 * `{d, time}`, `{n, plural, =0 {..} one {..} other {..}}` (with `#`), `offset:1`, `selectordinal`,
 * `{g, select, a {..} other {..}}`, and apostrophe quoting (`''` is a literal apostrophe, `'{'` a literal
 * brace). Nesting works. Anything malformed throws `IcuSyntaxError` at compile time so the catalog
 * test catches it before users do.
 */

export class IcuSyntaxError extends Error {}

export type Node =
  | { t: 'text'; v: string }
  | { t: 'arg'; name: string }
  | { t: 'number'; name: string; style?: string }
  | { t: 'date' | 'time'; name: string; style?: string }
  | { t: 'plural'; name: string; offset: number; ordinal: boolean; cases: Map<string, Node[]> }
  | { t: 'select'; name: string; cases: Map<string, Node[]> }
  | { t: 'pound' };

export type Values = Record<string, string | number | Date | boolean | null | undefined>;

class Parser {
  i = 0;
  constructor(private readonly s: string) {}

  parse(depth: number, plural: boolean): Node[] {
    const out: Node[] = [];
    let text = '';
    const flush = () => {
      if (text) out.push({ t: 'text', v: text });
      text = '';
    };
    while (this.i < this.s.length) {
      const c = this.s[this.i]!;
      if (c === "'") {
        const n = this.s[this.i + 1];
        if (n === "'") {
          text += "'";
          this.i += 2;
        } else if (n === '{' || n === '}' || (plural && n === '#')) {
          const end = this.s.indexOf("'", this.i + 1);
          if (end < 0) {
            text += this.s.slice(this.i + 1);
            this.i = this.s.length;
          } else {
            text += this.s.slice(this.i + 1, end).replace(/''/g, "'");
            this.i = end + 1;
          }
        } else {
          text += c;
          this.i++;
        }
      } else if (c === '{') {
        flush();
        this.i++;
        out.push(this.argument(depth));
      } else if (c === '}') {
        if (depth === 0) throw new IcuSyntaxError(`Unmatched '}' in "${this.s}"`);
        break;
      } else if (c === '#' && plural) {
        flush();
        out.push({ t: 'pound' });
        this.i++;
      } else {
        text += c;
        this.i++;
      }
    }
    flush();
    return out;
  }

  private ws() {
    while (/\s/.test(this.s[this.i] ?? '')) this.i++;
  }

  private ident(): string {
    this.ws();
    const m = /^[A-Za-z0-9_.-]+/.exec(this.s.slice(this.i));
    if (!m) throw new IcuSyntaxError(`Expected a name at ${this.i} in "${this.s}"`);
    this.i += m[0].length;
    return m[0];
  }

  private argument(depth: number): Node {
    const name = this.ident();
    this.ws();
    if (this.s[this.i] === '}') {
      this.i++;
      return { t: 'arg', name };
    }
    if (this.s[this.i] !== ',') throw new IcuSyntaxError(`Expected ',' or '}' in "${this.s}"`);
    this.i++;
    const kind = this.ident();
    this.ws();
    let style: string | undefined;
    if (this.s[this.i] === ',' && (kind === 'number' || kind === 'date' || kind === 'time')) {
      this.i++;
      this.ws();
      const end = this.s.indexOf('}', this.i);
      if (end < 0) throw new IcuSyntaxError(`Unclosed argument in "${this.s}"`);
      style = this.s.slice(this.i, end).trim();
      this.i = end;
    } else if (this.s[this.i] === ',') {
      this.i++;
    }
    if (kind === 'number' || kind === 'date' || kind === 'time') {
      this.ws();
      this.expect('}');
      return { t: kind, name, style };
    }
    if (kind === 'plural' || kind === 'selectordinal' || kind === 'select') {
      let offset = 0;
      const cases = new Map<string, Node[]>();
      for (;;) {
        this.ws();
        if (this.s[this.i] === '}') {
          this.i++;
          break;
        }
        const key = this.selector();
        if (key.startsWith('offset:')) {
          offset = Number(key.slice(7));
          continue;
        }
        this.ws();
        this.expect('{');
        cases.set(key, this.parse(depth + 1, kind !== 'select'));
        this.expect('}');
      }
      if (!cases.has('other'))
        throw new IcuSyntaxError(`"${kind}" needs an 'other' case in "${this.s}"`);
      return kind === 'select'
        ? { t: 'select', name, cases }
        : { t: 'plural', name, offset, ordinal: kind === 'selectordinal', cases };
    }
    throw new IcuSyntaxError(`Unknown argument type "${kind}" in "${this.s}"`);
  }

  private selector(): string {
    const m = /^(offset:\s*\d+|=\d+|[^\s{}]+)/.exec(this.s.slice(this.i));
    if (!m) throw new IcuSyntaxError(`Expected a case selector at ${this.i} in "${this.s}"`);
    this.i += m[0].length;
    return m[0].replace(/\s+/g, '');
  }

  private expect(ch: string) {
    if (this.s[this.i] !== ch)
      throw new IcuSyntaxError(`Expected '${ch}' at ${this.i} in "${this.s}"`);
    this.i++;
  }
}

export function parseIcu(message: string): Node[] {
  const p = new Parser(message);
  const nodes = p.parse(0, false);
  if (p.i < message.length) throw new IcuSyntaxError(`Unmatched '}' in "${message}"`);
  return nodes;
}

/** Names of every argument a message uses (for catalog checks). */
export function argumentNames(nodes: Node[], into = new Set<string>()): Set<string> {
  for (const n of nodes) {
    if (n.t === 'text' || n.t === 'pound') continue;
    into.add(n.name);
    if (n.t === 'plural' || n.t === 'select')
      for (const c of n.cases.values()) argumentNames(c, into);
  }
  return into;
}

const DATE_STYLES: Record<string, Intl.DateTimeFormatOptions> = {
  short: { dateStyle: 'short' },
  medium: { dateStyle: 'medium' },
  long: { dateStyle: 'long' },
  full: { dateStyle: 'full' },
};
const TIME_STYLES: Record<string, Intl.DateTimeFormatOptions> = {
  short: { timeStyle: 'short' },
  medium: { timeStyle: 'medium' },
  long: { timeStyle: 'long' },
};

export interface FormatOptions {
  /** Force 12 or 24 hour clocks; undefined follows the locale. */
  hour12?: boolean;
}

export function formatNodes(
  nodes: Node[],
  values: Values,
  locale: string,
  opts: FormatOptions = {},
  pound?: string,
): string {
  let out = '';
  for (const n of nodes) {
    switch (n.t) {
      case 'text':
        out += n.v;
        break;
      case 'pound':
        out += pound ?? '#';
        break;
      case 'arg': {
        const v = values[n.name];
        out +=
          v === undefined || v === null
            ? `{${n.name}}`
            : v instanceof Date
              ? v.toISOString()
              : String(v);
        break;
      }
      case 'number': {
        const o: Intl.NumberFormatOptions =
          n.style === 'percent'
            ? { style: 'percent' }
            : n.style === 'integer'
              ? { maximumFractionDigits: 0 }
              : {};
        out += new Intl.NumberFormat(locale, o).format(Number(values[n.name]));
        break;
      }
      case 'date':
      case 'time': {
        const raw = values[n.name];
        const d = raw instanceof Date ? raw : new Date(raw as string | number);
        const table = n.t === 'date' ? DATE_STYLES : TIME_STYLES;
        const base = table[n.style ?? 'medium'] ?? table.medium!;
        const hour = n.t === 'time' && opts.hour12 !== undefined ? { hour12: opts.hour12 } : {};
        out += new Intl.DateTimeFormat(locale, { ...base, ...hour }).format(d);
        break;
      }
      case 'select': {
        const key = String(values[n.name]);
        out += formatNodes(n.cases.get(key) ?? n.cases.get('other')!, values, locale, opts, pound);
        break;
      }
      case 'plural': {
        const num = Number(values[n.name]);
        const shifted = num - n.offset;
        const exact = n.cases.get(`=${num}`);
        const cat = new Intl.PluralRules(locale, {
          type: n.ordinal ? 'ordinal' : 'cardinal',
        }).select(shifted);
        const branch = exact ?? n.cases.get(cat) ?? n.cases.get('other')!;
        out += formatNodes(
          branch,
          values,
          locale,
          opts,
          new Intl.NumberFormat(locale).format(shifted),
        );
        break;
      }
    }
  }
  return out;
}

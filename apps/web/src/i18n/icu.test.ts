import { argumentNames, formatNodes, IcuSyntaxError, parseIcu } from './icu';
import { pseudoText, wrapPseudo } from './pseudo';

const f = (msg: string, v = {}, locale = 'en') => formatNodes(parseIcu(msg), v, locale);

describe('ICU subset', () => {
  it('substitutes arguments and leaves missing ones visible', () => {
    expect(f('Hi {name}!', { name: 'Ada' })).toBe('Hi Ada!');
    expect(f('Hi {name}!')).toBe('Hi {name}!');
  });

  it('handles plurals with exact matches and #', () => {
    const m = '{n, plural, =0 {No messages} one {# message} other {# messages}}';
    expect(f(m, { n: 0 })).toBe('No messages');
    expect(f(m, { n: 1 })).toBe('1 message');
    expect(f(m, { n: 1234 })).toBe('1,234 messages');
  });

  it('uses the locale plural rules (Arabic has zero, two, few and many)', () => {
    const m = '{n, plural, zero {z} one {o} two {tw} few {f} many {m} other {x}}';
    expect([0, 1, 2, 3, 11, 100].map((n) => f(m, { n }, 'ar'))).toEqual([
      'z',
      'o',
      'tw',
      'f',
      'm',
      'x',
    ]);
  });

  it('supports offset, select and selectordinal', () => {
    expect(
      f('{n, plural, offset:1 =0 {nobody} =1 {just you} other {you and # others}}', { n: 3 }),
    ).toBe('you and 2 others');
    expect(f('{g, select, f {she} m {he} other {they}}', { g: 'm' })).toBe('he');
    expect(f('{g, select, f {she} other {they}}', { g: 'zz' })).toBe('they');
    const ord = '{n, selectordinal, one {#st} two {#nd} few {#rd} other {#th}}';
    expect([1, 2, 3, 4, 11, 22].map((n) => f(ord, { n }))).toEqual([
      '1st',
      '2nd',
      '3rd',
      '4th',
      '11th',
      '22nd',
    ]);
  });

  it('formats numbers and dates by locale', () => {
    expect(f('{n, number}', { n: 1234567.5 }, 'en')).toBe('1,234,567.5');
    expect(f('{n, number}', { n: 1234567.5 }, 'de')).toBe('1.234.567,5');
    expect(f('{p, number, percent}', { p: 0.5 })).toBe('50%');
    const d = new Date(2026, 0, 31, 15, 0);
    expect(f('{d, date, long}', { d }, 'en')).toBe('January 31, 2026');
    expect(f('{d, time, short}', { d }, 'en')).toMatch(/3:00\s?PM/);
    expect(formatNodes(parseIcu('{d, time, short}'), { d }, 'en', { hour12: false })).toBe('15:00');
  });

  it('quotes braces and apostrophes', () => {
    expect(f("It's '{literal}' and don''t")).toBe("It's {literal} and don't");
    expect(f("{n, plural, other {'#' stays}}", { n: 2 })).toBe('# stays');
  });

  it('rejects malformed messages', () => {
    for (const bad of [
      '{',
      '{n, plural, one {x}}',
      '{n, bogus}',
      'a } b',
      '{n, plural, other {x}',
    ]) {
      expect(() => parseIcu(bad), bad).toThrow(IcuSyntaxError);
    }
  });

  it('lists argument names, including nested ones', () => {
    expect([...argumentNames(parseIcu('{a} {n, plural, one {{b}} other {# {c}}}'))].sort()).toEqual(
      ['a', 'b', 'c', 'n'],
    );
  });
});

describe('pseudo-localisation', () => {
  it('accents letters, grows the text and keeps arguments intact', () => {
    expect(pseudoText('Hello')).toMatch(/^Ĥéļļö·+$/);
    const nodes = wrapPseudo(parseIcu('Hi {name}, {n, plural, one {# item} other {# items}}'));
    const out = formatNodes(nodes, { name: 'Ada', n: 2 }, 'en');
    expect(out.startsWith('[')).toBe(true);
    expect(out.endsWith(']')).toBe(true);
    expect(out).toContain('Ada');
    expect(out).toContain('2 ');
    expect(out).toContain('îţéɱš');
  });
});

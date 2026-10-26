import { buildPseudoCatalog, pseudoMessage } from '../../../../scripts/pseudo-locale.mjs';
import { parseIcu, type Node } from './icu';
import { en } from './messages/en';
import './messages/en.rest';
import { wrapPseudo } from './pseudo';
import { LOCALES, PSEUDO_ENABLED, setLocale, t } from './runtime';

/** Merge adjacent text nodes and turn case maps into plain objects, so two parses compare structurally. */
function norm(nodes: Node[]): unknown[] {
  const out: unknown[] = [];
  for (const n of nodes) {
    const last = out[out.length - 1] as { t?: string; v?: string } | undefined;
    if (n.t === 'text' && last?.t === 'text') last.v += n.v;
    else if (n.t === 'plural' || n.t === 'select')
      out.push({
        ...n,
        cases: Object.fromEntries([...n.cases].map(([k, v]) => [k, norm(v)])),
      });
    else out.push({ ...n });
  }
  return out;
}

describe('en-XA build-time catalog (RDY-I18N-002)', () => {
  const generated = buildPseudoCatalog();

  it('covers every English key', () => {
    expect(Object.keys(generated).sort()).toEqual(Object.keys(en).sort());
  });

  it('matches the runtime transform node for node, for every key', () => {
    const bad: string[] = [];
    for (const [key, text] of Object.entries(en)) {
      const want = norm(wrapPseudo(parseIcu(text)));
      const got = norm(parseIcu(generated[key]!));
      if (JSON.stringify(want) !== JSON.stringify(got)) bad.push(key);
    }
    expect(bad).toEqual([]);
  });

  it('keeps arguments, plurals and quoting, brackets and lengthens the text', () => {
    expect(pseudoMessage("It's {n, plural, one {# day} other {# days}} '{'x'}'")).toBe(
      "[Îţ''š ··{n, plural, one {# ðàý··} other {# ðàýš··}} '{'ẋ'}'··]",
    );
    expect(pseudoMessage('Hello')).toBe('[Ĥéļļö··]');
  });

  it('is served through the locale registry and not persisted for a query preview', async () => {
    expect(PSEUDO_ENABLED).toBe(true);
    expect(LOCALES.find((l) => l.code === 'en-XA')?.generated).toBe(true);
    await setLocale('en-XA', false);
    expect(t('entrance.login.title')).toBe(generated['entrance.login.title']);
    expect(localStorage.getItem('sr.locale')).toBeNull();
    await setLocale('en');
  });
});

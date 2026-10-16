import { safeTokens, themeOf, themeStyle, useStoreThemes } from '../themes';

describe('store themes (EXT-PKG-005)', () => {
  it('keeps design tokens with plain values and drops everything else', () => {
    const t = safeTokens({
      '--ui-accent': '#5865f2',
      '--ui-bg': 'rgb(10, 20, 30)',
      '--ui-radius': '6px',
      '--ui-text': 'url(https://evil.example/x.png)',
      '--ui-border': '#fff; background: red',
      '--ui-danger': 'var(--x)',
      '--other': '#000',
      color: 'red',
      '--ui-font-body': 'x{}',
      '--ui-info': 42,
    });
    expect(t).toEqual({
      '--ui-accent': '#5865f2',
      '--ui-bg': 'rgb(10, 20, 30)',
      '--ui-radius': '6px',
    });
    expect(safeTokens(null)).toEqual({});
    expect(safeTokens('x')).toEqual({});
  });

  it('builds a theme from a package payload and a style from the theme', () => {
    const th = themeOf({
      id: '7',
      name: 'Ocean',
      section: 'cinema',
      payload: { scheme: 'light', tokens: { '--ui-accent': '#0af' } },
    });
    expect(th).toEqual({
      packageId: '7',
      name: 'Ocean',
      section: 'cinema',
      scheme: 'light',
      tokens: { '--ui-accent': '#0af' },
    });
    expect(themeStyle(th)).toEqual({ '--ui-accent': '#0af', colorScheme: 'light' });
    expect(themeStyle(null)).toEqual({});
    expect(themeOf({ name: 'x', payload: null })).toBeNull();
  });

  it('turns only theme enablements into themes', () => {
    const base = {
      version_id: '2',
      version: '1.0.0',
      section: 'all',
      config: {},
      enabled_at_ms: 0,
      update_available: false,
    };
    useStoreThemes.getState().setThemes([
      {
        ...base,
        package_id: '1',
        kind: 'theme',
        slug: 'a-theme',
        name: 'A',
        payload: { scheme: 'dark', tokens: { '--ui-bg': '#000' } },
      },
      {
        ...base,
        package_id: '3',
        kind: 'recipe',
        slug: 'a-recipe',
        name: 'R',
        payload: { features: [] },
      },
    ]);
    expect(useStoreThemes.getState().themes.map((t) => t.packageId)).toEqual(['1']);
    useStoreThemes.getState().setThemes([]);
  });
});

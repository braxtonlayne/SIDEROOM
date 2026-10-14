import { act, render, screen } from '@testing-library/react';
import { negotiateLocale, registerLocale, setLocale, t, unregisterLocale } from './runtime';
import { useLocale, useT } from './react';

afterEach(async () => {
  await setLocale('en');
  localStorage.clear();
});

function Probe() {
  const tr = useT();
  const { locale } = useLocale();
  return (
    <p data-testid="p">
      {locale}|{tr('entrance.login.title')}
    </p>
  );
}

describe('runtime', () => {
  it('translates and falls back to the key for unknown keys', () => {
    expect(t('entrance.login.title')).toBe('Come on in.');
    // @ts-expect-error not a catalog key
    expect(t('no.such.key')).toBe('no.such.key');
    expect(t('entrance.login.tooMany', { minutes: 1 })).toBe(
      'Too many attempts. Try again in 1 minute.',
    );
    expect(t('entrance.login.tooMany', { minutes: 5 })).toBe(
      'Too many attempts. Try again in 5 minutes.',
    );
  });

  it('switches locale, re-renders hook users, persists and sets lang/dir', async () => {
    render(<Probe />);
    expect(screen.getByTestId('p')).toHaveTextContent('en|Come on in.');
    await act(() => setLocale('ar-XB'));
    expect(screen.getByTestId('p').textContent).toMatch(/^ar-XB\|\[Çö/);
    expect(document.documentElement.dir).toBe('rtl');
    expect(document.documentElement.lang).toBe('ar');
    expect(localStorage.getItem('sr.locale')).toBe('ar-XB');
    await act(() => setLocale('en'));
    expect(document.documentElement.dir).toBe('ltr');
  });

  it('pseudo locale keeps plural arguments working', async () => {
    await setLocale('en-XA');
    const s = t('entrance.login.tooMany', { minutes: 3 });
    expect(s).toContain('3');
    expect(s.startsWith('[')).toBe(true);
  });

  it('loads a registered translation lazily and falls back per key', async () => {
    registerLocale({ code: 'xx', label: 'Test', dir: 'ltr' }, async () => ({
      'entrance.login.title': 'Translated!',
    }));
    await setLocale('xx');
    expect(t('entrance.login.title')).toBe('Translated!');
    expect(t('entrance.login.submit')).toBe('Step inside');
    await setLocale('en');
    unregisterLocale('xx');
  });

  it('negotiates from browser languages', () => {
    expect(negotiateLocale(['fr-CA', 'en-GB'])).toBe('en');
    expect(negotiateLocale([])).toBe('en');
  });
});

import { convertEmoticons } from '../../../lounge/emoticons';
import {
  comboFromEvent,
  comboKeys,
  defaultCombo,
  effectiveCombo,
  matchCustomShortcut,
  SHORTCUTS,
} from '../../../lounge/shortcuts';
import { comboProblem } from '../sections/Keybinds';
import { cropWindow, imageProblem } from '../imageCrop';
import { applyPrefs, DEFAULT_PREFS, isCombo, prefsEqual, sanitizePrefs } from '../prefs';
import { prefsPatch } from '../prefsSync';

describe('sanitizePrefs', () => {
  it('turns anything into valid preferences', () => {
    expect(sanitizePrefs(null)).toEqual(DEFAULT_PREFS);
    expect(sanitizePrefs('x')).toEqual(DEFAULT_PREFS);
    const p = sanitizePrefs({
      theme: 'lounge-onyx',
      sectionThemes: { library: 'sideroom', cinema: 'nope', bogus: 'lounge-dark', lounge: null },
      messageGap: 999,
      locale: 'de-CH',
      timeFormat: '13h',
      pushInactiveMinutes: 7,
      roleColors: 'dot',
      keybinds: { quickSwitcher: 'Mod+J', shortcutSheet: '', userSettings: 'bad key', x: 3 },
    });
    expect(p).toMatchObject({
      theme: 'lounge-onyx',
      sectionThemes: { library: 'sideroom' },
      messageGap: 24,
      locale: 'de-CH',
      timeFormat: 'auto',
      pushInactiveMinutes: 10,
      roleColors: 'dot',
      keybinds: { quickSwitcher: 'Mod+J', shortcutSheet: '' },
    });
  });

  it('compares by value', () => {
    expect(prefsEqual(sanitizePrefs({}), DEFAULT_PREFS)).toBe(true);
    expect(prefsEqual(sanitizePrefs({ keybinds: { a: 'Mod+J' } }), DEFAULT_PREFS)).toBe(false);
  });

  it('accepts only well-formed combos', () => {
    for (const ok of ['Mod+K', 'Mod+Alt+ArrowUp', 'F5', 'Mod+,', 'Alt+Shift+/', 'Mod++'])
      expect(isCombo(ok), ok).toBe(true);
    for (const bad of ['', 'Mod+', 'Mod + K', 'K+Mod', 3, null, 'Mod+K+J'])
      expect(isCombo(bad), String(bad)).toBe(false);
  });

  it('maps preferences to ThemeProvider props', () => {
    const p = applyPrefs(
      sanitizePrefs({ theme: 'system', zoom: 120, highContrast: true, underlineLinks: true }),
      false,
    );
    expect(p.userGlobal).toBe('lounge-light');
    expect(p.appearance.contrast).toBe('high');
    expect(p.style).toMatchObject({ zoom: 1.2, '--lng-message-gap': '16px' });
    expect(p.data['data-underline-links']).toBe('true');
    expect(p.data['data-dyslexia']).toBeUndefined();
    expect(applyPrefs(DEFAULT_PREFS, true).style).not.toHaveProperty('zoom');
  });
});

describe('prefsPatch', () => {
  it('nulls entries the account has that the new value dropped', () => {
    const next = sanitizePrefs({ keybinds: { a1: 'Mod+J' }, sectionThemes: {} });
    const patch = prefsPatch(
      { keybinds: { a1: 'Mod+J', gone: 'Mod+K' }, sectionThemes: { library: 'sideroom' } },
      next,
    );
    expect(patch.keybinds).toEqual({ a1: 'Mod+J', gone: null });
    expect((patch.sectionThemes as Record<string, unknown>).library).toBeNull();
    expect(prefsPatch({ keybinds: { gone: 'Mod+K' } }, next).keybinds).toEqual({
      a1: 'Mod+J',
      gone: null,
    });
    expect(prefsPatch(undefined, DEFAULT_PREFS).keybinds).toEqual({});
  });
});

describe('cropWindow', () => {
  it('takes the largest centred window of the aspect, then zooms in', () => {
    expect(cropWindow(1000, 500, 1, 1, 0.5, 0.5)).toEqual({ sx: 250, sy: 0, sw: 500, sh: 500 });
    expect(cropWindow(500, 1000, 1, 1, 0.5, 0.5)).toEqual({ sx: 0, sy: 250, sw: 500, sh: 500 });
    expect(cropWindow(1000, 1000, 2.5, 1, 0.5, 0)).toEqual({ sx: 0, sy: 0, sw: 1000, sh: 400 });
    const z = cropWindow(1000, 500, 1, 2, 0, 1);
    expect(z).toEqual({ sx: 0, sy: 250, sw: 250, sh: 250 });
  });

  it('clamps pan and zoom', () => {
    const w = cropWindow(1000, 500, 1, 0.1, 5, -5);
    expect(w).toEqual({ sx: 500, sy: 0, sw: 500, sh: 500 });
  });
});

describe('imageProblem', () => {
  it('accepts the four formats up to 8 MiB', () => {
    expect(imageProblem({ type: 'image/png', size: 10 })).toBeNull();
    expect(imageProblem({ type: 'image/webp', size: 8 * 1024 * 1024 })).toBeNull();
    expect(imageProblem({ type: 'image/svg+xml', size: 10 })).toMatch(/PNG, JPEG, GIF or WebP/);
    expect(imageProblem({ type: 'image/jpeg', size: 8 * 1024 * 1024 + 1 })).toMatch(/8 MiB/);
  });
});

describe('convertEmoticons', () => {
  it('converts standalone emoticons only', () => {
    expect(convertEmoticons('hello :) friend')).toBe('hello 🙂 friend');
    expect(convertEmoticons('<3')).toBe('❤️');
    expect(convertEmoticons('so :D and ;)')).toBe('so 😄 and 😉');
    expect(convertEmoticons('see http://x.test and a:b)')).toBe('see http://x.test and a:b)');
    expect(convertEmoticons('`:)` in code')).toBe('`:)` in code');
    expect(convertEmoticons('plain')).toBe('plain');
  });
});

describe('custom keybinds', () => {
  const ev = (key: string, mods: Partial<KeyboardEventInit> = {}) => ({
    key,
    ctrlKey: false,
    metaKey: false,
    altKey: false,
    shiftKey: false,
    ...mods,
  });

  it('derives combos from events and defaults', () => {
    expect(comboFromEvent(ev('k', { ctrlKey: true }))).toBe('Mod+K');
    expect(comboFromEvent(ev('ArrowUp', { altKey: true, shiftKey: true }))).toBe(
      'Alt+Shift+ArrowUp',
    );
    expect(comboFromEvent(ev('Control', { ctrlKey: true }))).toBeNull();
    expect(comboFromEvent(ev(' ', { altKey: true }))).toBe('Alt+Space');
    const nextGuild = SHORTCUTS.find((s) => s.id === 'nextGuild')!;
    expect(defaultCombo(nextGuild)).toBe('Mod+Alt+ArrowDown');
    expect(comboKeys('Mod+Alt+ArrowDown')).toEqual([expect.any(String), 'Alt', '↓']);
    expect(comboKeys('')).toEqual([]);
  });

  it('matches defaults, honours overrides and unbinding', () => {
    expect(matchCustomShortcut(ev('k', { ctrlKey: true }), {})).toBe('quickSwitcher');
    expect(matchCustomShortcut(ev('k', { metaKey: true }), {})).toBe('quickSwitcher');
    expect(matchCustomShortcut(ev('k', { ctrlKey: true }), { quickSwitcher: 'Mod+J' })).toBeNull();
    expect(matchCustomShortcut(ev('j', { ctrlKey: true }), { quickSwitcher: 'Mod+J' })).toBe(
      'quickSwitcher',
    );
    expect(matchCustomShortcut(ev('k', { ctrlKey: true }), { quickSwitcher: '' })).toBeNull();
    expect(matchCustomShortcut(ev('x'), {})).toBe('focusComposer');
    expect(matchCustomShortcut(ev('Escape'), {})).toBe('markRead');
    expect(effectiveCombo(SHORTCUTS[0]!, { quickSwitcher: 'F5' })).toBe('F5');
    expect(matchCustomShortcut(ev('F5'), { quickSwitcher: 'F5' })).toBe('quickSwitcher');
  });

  it('rejects bare letters, Escape and duplicates', () => {
    expect(comboProblem('K', 'prevGuild', {})).toMatch(/Hold Ctrl/);
    expect(comboProblem('Escape', 'prevGuild', {})).toMatch(/reserved|Hold Ctrl/);
    expect(comboProblem('Mod+K', 'prevGuild', {})).toMatch(/Already used/);
    expect(comboProblem('Mod+K', 'quickSwitcher', {})).toBeNull();
    expect(comboProblem('Mod+J', 'prevGuild', {})).toBeNull();
    expect(comboProblem('F6', 'prevGuild', {})).toBeNull();
    expect(comboProblem('Mod+K', 'prevGuild', { quickSwitcher: 'Mod+J' })).toBeNull();
  });
});

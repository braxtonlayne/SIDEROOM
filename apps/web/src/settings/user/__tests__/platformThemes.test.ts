import { resolveTheme } from '@sideroom/ui';
import { DEFAULT_PREFS, applyPrefs } from '../prefs';
import { parsePlatformThemes, withPlatformDefaults } from '../platformThemes';

describe('platform default themes (ADM-PLT-004)', () => {
  it('maps the server sections to the theme runtime and drops unknown ones', () => {
    expect(
      parsePlatformThemes({
        default_theme_by_section: {
          cinema: 'lounge-onyx',
          tv: 'sideroom',
          library: 'neon',
          nowhere: 'sideroom',
        },
      }),
    ).toEqual({ cinema: 'lounge-onyx', 'live-tv': 'sideroom' });
    expect(parsePlatformThemes(null)).toEqual({});
    expect(parsePlatformThemes({ default_theme_by_section: 3 })).toEqual({});
  });

  it('resolves own section choice, then own global choice, then the platform default', () => {
    const platform = parsePlatformThemes({
      default_theme_by_section: { cinema: 'lounge-onyx', library: 'lounge-ash' },
    });
    const pick = (
      prefs: Partial<typeof DEFAULT_PREFS>,
      section: 'cinema' | 'library' | 'support',
    ) => {
      const applied = applyPrefs({ ...DEFAULT_PREFS, ...prefs }, false, platform);
      return resolveTheme({
        section,
        userGlobal: applied.userGlobal,
        perSection: applied.perSection,
      });
    };
    // Nothing chosen: the platform default where there is one.
    expect(pick({}, 'cinema')).toBe('lounge-onyx');
    expect(pick({}, 'library')).toBe('lounge-ash');
    // A section without a platform default keeps what it had.
    expect(pick({}, 'support')).toBe('lounge-dark');
    // An own per-section choice wins over the platform default.
    expect(pick({ sectionThemes: { cinema: 'sideroom' } }, 'cinema')).toBe('sideroom');
    expect(pick({ sectionThemes: { cinema: 'sideroom' } }, 'library')).toBe('lounge-ash');
    // An own theme for everything wins over the platform default.
    expect(pick({ theme: 'lounge-light' }, 'cinema')).toBe('lounge-light');
  });

  it('leaves the viewer alone when there is no platform default', () => {
    expect(withPlatformDefaults({ lounge: 'sideroom' }, false, {})).toEqual({ lounge: 'sideroom' });
  });
});

import { renderHook, waitFor } from '@testing-library/react';
import { http, HttpResponse, server } from '../../test/server';
import { emitDispatch } from '../../store/events';
import {
  accentOfHex,
  hexOfAccent,
  useGuildAppearances,
  useServerAppearance,
} from '../serverAppearance';

describe('server appearance (LNG-SSET-050)', () => {
  beforeEach(() => useGuildAppearances.setState({ byGuild: {} }));

  it('converts accent numbers and hex both ways and refuses junk', () => {
    expect(hexOfAccent(0x336699)).toBe('#336699');
    expect(hexOfAccent(255)).toBe('#0000ff');
    expect(hexOfAccent(-1)).toBeNull();
    expect(hexOfAccent(null)).toBeNull();
    expect(accentOfHex('#336699')).toBe(0x336699);
    expect(accentOfHex('red')).toBeNull();
  });

  it('applies the theme tokens (checked again) and the accent, reads again on the gateway event', async () => {
    let accent = 0x336699;
    server.use(
      http.get('*/api/v1/guilds/g1/appearance', () =>
        HttpResponse.json({
          accent_color: accent,
          theme: {
            package_id: '1',
            version_id: '2',
            slug: 'dusk',
            name: 'Dusk',
            scheme: 'dark',
            tokens: { '--ui-bg': '#222', '--ui-evil': 'url(x)', bad: '1' },
          },
        }),
      ),
    );
    const { result } = renderHook(() => useServerAppearance('g1', true));
    await waitFor(() => expect(result.current.accent).toBe('#336699'));
    const style = result.current.style as Record<string, string>;
    expect(style['--ui-bg']).toBe('#222');
    expect(style['--ui-evil']).toBeUndefined();
    expect(style.bad).toBeUndefined();
    expect(style['--ui-accent']).toBe('#336699');
    accent = 0x112233;
    emitDispatch('SR_GUILD_APPEARANCE_UPDATE', { guild_id: 'g1' });
    await waitFor(() => expect(result.current.accent).toBe('#112233'));
  });

  it('applies nothing when the viewer turned server themes off or is on Home', async () => {
    server.use(
      http.get('*/api/v1/guilds/g1/appearance', () =>
        HttpResponse.json({ accent_color: 255, theme: null }),
      ),
    );
    const off = renderHook(() => useServerAppearance('g1', false));
    const home = renderHook(() => useServerAppearance(undefined, true));
    await new Promise((r) => setTimeout(r, 30));
    expect(off.result.current).toEqual({ accent: null, style: {} });
    expect(home.result.current).toEqual({ accent: null, style: {} });
  });
});

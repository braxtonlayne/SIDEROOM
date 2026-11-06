// ARC-UI-010: the Arcade page in the app router: the rooms' chrome (the SIDE ROOM bar with the Arcade current), the
// Featured section and its jump link only when the Operator picked something (D-474), and the provenance line in
// the Featured intro (D-475).
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { createAppRouter } from '../../router';
import { DEFAULT_PREFS, usePrefsStore, type UserPrefs } from '../../settings/user/prefs';
import { useStore } from '../../store/store';
import { http, HttpResponse, server } from '../../test/server';
import { ArcadeThemeProvider } from '../ArcadeTheme';
import type { ArcadeFront, FeaturedCard } from '../types';

const pick: FeaturedCard = {
  application_id: '10',
  name: 'Orbit',
  description: 'A community game',
  blurb: null,
  tier: 'TIER_1_ORIGINAL',
  eligibility: {
    browser_play: true,
    guest_links: true,
    featured: true,
    desktop_only: false,
    needs_base_game_hint: false,
  },
  capabilities: [],
  tip_links: [],
  bundle_sha256: 'a'.repeat(64),
  code_sha256: 'c'.repeat(64),
  position: 0,
  featured_at_ms: 1,
};

function serve(front: ArcadeFront, lobbies: unknown[] = []) {
  server.use(
    http.get('*/api/v1/users/@me/activity-lobbies', () =>
      HttpResponse.json({ enabled: true, items: lobbies }),
    ),
    http.get('*/api/v1/users/@me', () => HttpResponse.json({ id: '1', username: 'me' })),
    http.get('*/api/v1/arcade', () => HttpResponse.json(front)),
    http.get('*/api/v1/guilds/:id/arcade/cartridges', () => HttpResponse.json([])),
  );
}

function open() {
  const router = createAppRouter(createMemoryHistory({ initialEntries: ['/games'] }));
  return render(<RouterProvider router={router} />);
}

beforeEach(() => {
  useStore.setState({
    currentUserId: '1',
    users: { '1': { id: '1', username: 'me' } },
    guilds: { '70': { id: '70', name: 'Club' } },
  } as never);
});

describe('the Arcade page', () => {
  it('sits in the rooms’ chrome with the Arcade as the current room', async () => {
    serve({ featured: [], coming_soon: [], flags: [] });
    open();
    const rooms = await screen.findByTestId('rooms-nav');
    expect(within(rooms).getByRole('link', { name: 'The Arcade' })).toHaveAttribute(
      'aria-current',
      'page',
    );
    expect(within(rooms).getByRole('link', { name: 'The Lounge' })).not.toHaveAttribute(
      'aria-current',
    );
    expect(screen.getByRole('heading', { level: 1, name: 'The Arcade.' })).toBeInTheDocument();
  });

  it('hides Featured and its jump link while the Operator has picked nothing (D-474)', async () => {
    serve({ featured: [], coming_soon: [], flags: [] });
    open();
    await screen.findByRole('heading', { name: 'Server cartridges' });
    expect(screen.queryByRole('heading', { name: 'Featured' })).toBeNull();
    const nav = screen.getByRole('navigation', { name: 'Arcade sections' });
    expect(within(nav).queryByRole('link', { name: 'Featured' })).toBeNull();
    expect(within(nav).getByRole('link', { name: 'Playable now' })).toHaveAttribute(
      'href',
      '#playable',
    );
  });

  it('shows Featured, its jump link and the provenance line once there is a pick (D-475)', async () => {
    serve({ featured: [pick], coming_soon: [], flags: [] });
    open();
    expect(await screen.findByRole('heading', { name: 'Featured' })).toBeInTheDocument();
    const nav = screen.getByRole('navigation', { name: 'Arcade sections' });
    expect(within(nav).getByRole('link', { name: 'Featured' })).toHaveAttribute(
      'href',
      '#featured',
    );
    expect(
      screen.getByText(/Each one says who owns its code, art and sound\./),
    ).toBeInTheDocument();
    // D-477: the pick is also the first large card; here, its Featured card.
    expect(
      within(screen.getByRole('region', { name: 'Featured' })).getByRole('article', {
        name: 'Orbit',
      }),
    ).toBeInTheDocument();
  });

  it('puts Active lobbies between the hero and Playable now once a game runs (ARC-LOB-001)', async () => {
    serve({ featured: [], coming_soon: [], flags: [] }, [
      {
        instance_id: '501',
        application_id: '42',
        application_name: 'Land.io',
        guild_id: '70',
        guild_name: 'Club',
        channel_id: '9',
        channel_name: 'general',
        channel_type: 'text',
        development: false,
        player_count: 2,
        created_at_ms: Date.now(),
        joined: false,
      },
    ]);
    open();
    const lobbies = await screen.findByRole('heading', { name: 'Active lobbies' });
    const hero = screen.getByRole('heading', { level: 1, name: 'The Arcade.' });
    const playable = screen.getByRole('heading', { name: 'Playable now' });
    expect(hero.compareDocumentPosition(lobbies) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(
      lobbies.compareDocumentPosition(playable) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    const nav = screen.getByRole('navigation', { name: 'Arcade sections' });
    expect(within(nav).getByRole('link', { name: 'Active lobbies' })).toHaveAttribute(
      'href',
      '#lobbies',
    );
  });

  it('wraps the page in the arcade section (D-476)', async () => {
    serve({ featured: [], coming_soon: [], flags: [] });
    open();
    await waitFor(() => expect(document.querySelector('[data-section="arcade"]')).not.toBeNull());
  });
});

describe('D-476: the Arcade is an editorial room', () => {
  const themeOf = (prefs: UserPrefs) => {
    act(() => usePrefsStore.setState({ prefs }));
    const { container, unmount } = render(
      <ArcadeThemeProvider>
        <p>room</p>
      </ArcadeThemeProvider>,
    );
    const theme = container.querySelector('[data-section="arcade"]')?.getAttribute('data-theme');
    unmount();
    return theme;
  };
  afterEach(() => act(() => usePrefsStore.setState({ prefs: DEFAULT_PREFS })));

  it('is editorial whatever the member’s theme for everything', () => {
    expect(themeOf(DEFAULT_PREFS)).toBe('lounge-editorial');
    expect(themeOf({ ...DEFAULT_PREFS, theme: 'lounge-light' })).toBe('lounge-editorial');
    expect(themeOf({ ...DEFAULT_PREFS, theme: 'lounge-dark' })).toBe('lounge-editorial');
  });

  it('follows a theme the member chose for the Arcade', () => {
    expect(
      themeOf({
        ...DEFAULT_PREFS,
        theme: 'lounge-light',
        sectionThemes: { arcade: 'lounge-dark' },
      }),
    ).toBe('lounge-dark');
  });
});

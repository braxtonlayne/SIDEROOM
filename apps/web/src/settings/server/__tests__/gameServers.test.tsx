// GSV-PAGE-001: the cosmetic Game Servers page, its switch, its nav entry and the Activities tile.
import { ThemeProvider } from '@sideroom/ui';
import { render, screen, within } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { ActivityLauncher } from '../../../activities/ActivityLauncher';
import type { ActivityApi } from '../../../activities/api';
import { setClientFlags } from '../../../moderation/reports';
import { GameServersSection } from '../gameservers/GameServersSection';
import { SECTIONS } from '../ServerSettings';
import { MANAGE, openSettings, seed } from './helpers';

vi.mock('../../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const viewer = (can: boolean) => ({ ready: true, isOwner: false, can: () => can }) as never;

function motion(reduce: boolean) {
  window.matchMedia = ((query: string) => ({
    matches: reduce && query.includes('prefers-reduced-motion'),
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false,
    onchange: null,
  })) as unknown as typeof window.matchMedia;
}

afterEach(() => setClientFlags({}));

describe('the Game Servers page', () => {
  it('renders the splash and both cards, all inert', async () => {
    motion(false);
    const { container } = render(
      <ThemeProvider section="lounge">
        <GameServersSection />
      </ThemeProvider>,
    );
    expect(screen.getByRole('heading', { level: 1, name: 'Game Servers' })).toBeInTheDocument();
    expect(screen.getByText('UNDER CONSTRUCTION')).toBeInTheDocument();
    expect(screen.getByRole('heading', { name: 'COMING SOON' })).toBeInTheDocument();
    const byoc = screen.getByRole('region', { name: 'Host it yourself (BYOC)' });
    const managed = screen.getByRole('region', { name: 'Managed servers' });
    for (const card of [byoc, managed]) {
      expect(within(card).getByText('Coming soon')).toBeInTheDocument();
      expect(within(card).getByText('Preview only. Nothing on this card works yet.')).toBeVisible();
    }
    expect(screen.getByRole('img', { name: /Visitor counter/ })).toBeInTheDocument();
    // No form, no button, no link, no input anywhere on the page.
    expect(container.querySelectorAll('button, a, input, select, textarea, form')).toHaveLength(0);
    expect(await axeViolations(container)).toEqual([]);
  });

  it('scrolls the marquee normally and stands it still under prefers-reduced-motion', () => {
    motion(false);
    const { unmount } = render(<GameServersSection />);
    expect(screen.getByTestId('gsv-marquee')).toHaveAttribute('data-motion', 'full');
    unmount();
    motion(true);
    render(<GameServersSection />);
    expect(screen.getByTestId('gsv-marquee')).toHaveAttribute('data-motion', 'reduced');
  });
});

describe('the Game Servers nav entry (platform.game_servers)', () => {
  const entry = SECTIONS.find((s) => s.id === 'gameservers')!;

  it('is a MANAGE_GUILD entry of the Apps group, hidden only by an explicit off', () => {
    expect(entry.group).toBe('APPS');
    expect(entry.visible(viewer(true), { bridgeOn: false })).toBe(true);
    expect(entry.visible(viewer(true), { bridgeOn: false, gameServersOn: true })).toBe(true);
    expect(entry.visible(viewer(true), { bridgeOn: false, gameServersOn: false })).toBe(false);
    expect(entry.visible(viewer(false), { bridgeOn: false })).toBe(false);
  });

  it('shows in the settings nav and opens the page while the feature is on', async () => {
    motion(false);
    seed({ perms: [MANAGE.guild] });
    await openSettings('gameservers');
    expect(await screen.findByRole('heading', { name: 'COMING SOON' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Game Servers' })).toBeInTheDocument();
  });

  it('is gone from the nav and the route renders the not-found view when the feature is off', async () => {
    motion(false);
    setClientFlags({ 'platform.game_servers': false });
    seed({ perms: [MANAGE.guild] });
    await openSettings('gameservers');
    expect(await screen.findByText('Page not found.')).toBeInTheDocument();
    expect(screen.queryByRole('heading', { name: 'COMING SOON' })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Game Servers' })).not.toBeInTheDocument();
  });
});

describe('the Activities picker tile', () => {
  const api = {
    shelf: vi.fn(async () => ({ enabled: true, items: [] })),
  } as unknown as ActivityApi;
  const open = () =>
    render(
      <ThemeProvider section="lounge">
        <ActivityLauncher open onClose={() => undefined} channelId="3" api={api} />
      </ThemeProvider>,
    );

  it('says game servers are coming soon, with both tiers and nothing to press', async () => {
    const { baseElement } = open();
    const tile = await screen.findByRole('complementary', { name: 'Game servers: coming soon' });
    expect(within(tile).getByText('Host it yourself (BYOC)')).toBeInTheDocument();
    expect(within(tile).getByText('Managed servers')).toBeInTheDocument();
    expect(within(tile).queryByRole('button')).not.toBeInTheDocument();
    expect(await axeViolations(baseElement)).toEqual([]);
  });

  it('is hidden when the feature is off', async () => {
    setClientFlags({ 'platform.game_servers': false });
    open();
    expect(await screen.findByText('No Activities are available yet.')).toBeInTheDocument();
    expect(screen.queryByText('Game servers: coming soon')).not.toBeInTheDocument();
  });
});

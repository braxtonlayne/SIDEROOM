// LNG-RAIL-014: the rail's NEW badge comes from the member's own joined_at and goes when the server is opened.
import { screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { useStore } from '../../store';
import { warmLoungeRoute } from '../../test/warmRoutes';
import { isNewServer, resetOpenedServers } from '../newServer';
import { at, baseHandlers, seedLounge } from './gapsHelpers';

warmLoungeRoute();
vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const H = 3_600_000;

describe('isNewServer', () => {
  const now = Date.parse('2026-11-13T12:00:00Z');
  it('is new for 24 hours after joining, until opened', () => {
    expect(isNewServer(new Date(now - 2 * H).toISOString(), false, now)).toBe(true);
    expect(isNewServer(new Date(now - 23 * H).toISOString(), false, now)).toBe(true);
    expect(isNewServer(new Date(now - 25 * H).toISOString(), false, now)).toBe(false);
    expect(isNewServer(new Date(now - 2 * H).toISOString(), true, now)).toBe(false);
    expect(isNewServer(undefined, false, now)).toBe(false);
    expect(isNewServer('not a date', false, now)).toBe(false);
  });
});

describe('rail badge', () => {
  beforeEach(() => {
    localStorage.clear();
    resetOpenedServers();
    baseHandlers();
    seedLounge({ messages: [] });
    useStore.setState((s) => ({
      guilds: { ...s.guilds, g2: { id: 'g2', name: 'Fresh Guild', owner_id: '2' } },
      guildOrder: ['g1', 'g2'],
      channels: {
        ...s.channels,
        d1: { id: 'd1', type: 0, guild_id: 'g2', name: 'lobby', position: 0 },
      },
      channelIdsByGuild: { ...s.channelIdsByGuild, g2: ['d1'] },
      members: {
        ...s.members,
        'g1:1': { guild_id: 'g1', user_id: '1', roles: [], joined_at: '2020-01-01T00:00:00Z' },
        'g2:1': {
          guild_id: 'g2',
          user_id: '1',
          roles: [],
          joined_at: new Date(Date.now() - 2 * H).toISOString(),
        },
      },
    }));
  });

  it('marks a server joined today, and drops the mark once it is opened', async () => {
    at('/channels/g1/c1');
    const rail = await screen.findByRole('navigation', { name: 'Servers' });
    const fresh = await within(rail).findByRole('link', { name: /Fresh Guild/ });
    expect(within(fresh).getByText('NEW')).toBeInTheDocument();
    expect(within(rail).getByRole('link', { name: /Store Guild/ }).textContent).not.toContain(
      'NEW',
    );
    await userEvent.click(fresh);
    await screen.findAllByText('lobby');
    expect(within(rail).getByRole('link', { name: /Fresh Guild/ }).textContent).not.toContain(
      'NEW',
    );
    expect(localStorage.getItem('sr.lounge.openedServers')).toContain('g2');
  });
});

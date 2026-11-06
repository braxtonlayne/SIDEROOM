// ARC-LOB-001 (D-483 to D-487): the Arcade's "Active lobbies" strip. Hidden with no lobbies or a failed fetch;
// with lobbies it sits between the hero and Playable now, its pill link appears, Join joins (a voice lobby's call
// first) and goes to the channel, Return only goes there, a 404 says the game ended and refetches, an instance
// event refetches once within a second, and the built-in cards count their lobbies ("N live").
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { LobbyApi } from '../../activities/api';
import type { Lobbies, Lobby } from '../../activities/types';
import { ApiError } from '../../api/client';
import { emitDispatch } from '../../store/events';
import { IDLE_SESSION, useVoiceSession } from '../../voice/session';
import type { ArcadeApi } from '../api';
import { ArcadePage } from '../ArcadePage';
import { LOBBY_CLOCK_MS, LobbiesSection } from '../ArcadeSections';
import { liveCount } from '../builtins';
import type { TrustSource } from '../trust';

const navigate = vi.fn();
vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => navigate,
  Link: ({ to, children, ...rest }: { to: string; children: ReactNode; className?: string }) => (
    <a href={to} {...rest}>
      {children}
    </a>
  ),
}));

const arcadeApi: ArcadeApi = {
  front: () => Promise.resolve({ featured: [], coming_soon: [], flags: [] }),
  setNotify: () => Promise.resolve(),
  cartridges: () => Promise.resolve([]),
  addCartridge: () => Promise.reject(new Error('no')),
  removeCartridge: () => Promise.resolve(),
};

const trust: TrustSource = { badges: () => [] };

function lobby(over: Partial<Lobby> = {}): Lobby {
  return {
    instance_id: '501',
    application_id: '42',
    application_name: 'Land.io',
    guild_id: '70',
    guild_name: 'Club',
    channel_id: '9',
    channel_name: 'general',
    channel_type: 'text',
    development: false,
    player_count: 3,
    created_at_ms: Date.now() - 12 * 60_000,
    joined: false,
    ...over,
  };
}

function lobbyApi(answer: () => Promise<Lobbies>, over: Partial<LobbyApi> = {}) {
  return {
    lobbies: vi.fn(answer),
    join: vi.fn(() => Promise.resolve({} as never)),
    ...over,
  } satisfies LobbyApi;
}

function page(api: LobbyApi, callJoin = vi.fn(() => Promise.resolve(true))) {
  render(<ArcadePage api={arcadeApi} trust={trust} lobbyApi={api} callJoin={callJoin} />);
  return callJoin;
}

afterEach(() => {
  act(() => useVoiceSession.setState(IDLE_SESSION));
  navigate.mockReset();
  vi.useRealTimers();
});

describe('Active lobbies', () => {
  it('is absent with no lobbies, and its pill link too', async () => {
    const api = lobbyApi(() => Promise.resolve({ enabled: true, items: [] }));
    page(api);
    await waitFor(() => expect(api.lobbies).toHaveBeenCalled());
    await screen.findByRole('heading', { name: 'Server cartridges' });
    expect(screen.queryByRole('heading', { name: 'Active lobbies' })).toBeNull();
    const nav = screen.getByRole('navigation', { name: 'Arcade sections' });
    expect(within(nav).queryByRole('link', { name: 'Active lobbies' })).toBeNull();
  });

  it('is absent when the fetch fails', async () => {
    const api = lobbyApi(() => Promise.reject(new Error('down')));
    page(api);
    await waitFor(() => expect(api.lobbies).toHaveBeenCalled());
    await screen.findByRole('heading', { name: 'Server cartridges' });
    expect(screen.queryByRole('heading', { name: 'Active lobbies' })).toBeNull();
  });

  it('sits between the hero and Playable now, with its pill link, and names nobody', async () => {
    const api = lobbyApi(() =>
      Promise.resolve({
        enabled: true,
        items: [lobby(), lobby({ instance_id: '502', channel_type: 'voice', channel_name: 'Den' })],
      }),
    );
    page(api);
    const heading = await screen.findByRole('heading', { name: 'Active lobbies' });
    const hero = screen.getByRole('heading', { level: 1, name: 'The Arcade.' });
    const playable = screen.getByRole('heading', { name: 'Playable now' });
    expect(hero.compareDocumentPosition(heading) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(
      heading.compareDocumentPosition(playable) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    const nav = screen.getByRole('navigation', { name: 'Arcade sections' });
    expect(within(nav).getByRole('link', { name: 'Active lobbies' })).toHaveAttribute(
      'href',
      '#lobbies',
    );
    const list = screen.getByRole('list', { name: 'Games you can join' });
    expect(within(list).getAllByRole('listitem')).toHaveLength(2);
    expect(within(list).getByText('Club · #general')).toBeInTheDocument();
    expect(within(list).getByLabelText('Club · Den, a voice channel')).toBeInTheDocument();
    expect(within(list).getAllByText('3 playing')).toHaveLength(2);
    expect(within(list).getAllByText('for 12 min')).toHaveLength(2);
  });

  it('joins a text lobby, then goes to its channel', async () => {
    const user = userEvent.setup();
    const api = lobbyApi(() => Promise.resolve({ enabled: true, items: [lobby()] }));
    const call = page(api);
    await user.click(await screen.findByRole('button', { name: 'Join Land.io in general' }));
    await waitFor(() => expect(navigate).toHaveBeenCalled());
    expect(api.join).toHaveBeenCalledWith('501');
    expect(call).not.toHaveBeenCalled();
    expect(navigate).toHaveBeenCalledWith({
      to: '/channels/$guildId/$channelId',
      params: { guildId: '70', channelId: '9' },
    });
  });

  it('joins a voice lobby’s call first, and never the instance when the call fails', async () => {
    const user = userEvent.setup();
    const api = lobbyApi(() =>
      Promise.resolve({
        enabled: true,
        items: [lobby({ channel_type: 'voice', channel_name: 'Den', channel_id: '8' })],
      }),
    );
    const call = page(
      api,
      vi.fn(() => Promise.resolve(false)),
    );
    await user.click(await screen.findByRole('button', { name: 'Join Land.io in Den' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('That call could not be joined.');
    expect(call).toHaveBeenCalledWith({ guildId: '70', channelId: '8', name: 'Den' });
    expect(api.join).not.toHaveBeenCalled();
    expect(navigate).not.toHaveBeenCalled();
  });

  it('joins a voice lobby after its call connects', async () => {
    const user = userEvent.setup();
    const order: string[] = [];
    const api = lobbyApi(
      () =>
        Promise.resolve({
          enabled: true,
          items: [lobby({ channel_type: 'voice', channel_name: 'Den', channel_id: '8' })],
        }),
      {
        join: vi.fn(() => {
          order.push('instance');
          return Promise.resolve({} as never);
        }),
      },
    );
    page(
      api,
      vi.fn(() => {
        order.push('call');
        return Promise.resolve(true);
      }),
    );
    await user.click(await screen.findByRole('button', { name: 'Join Land.io in Den' }));
    await waitFor(() => expect(navigate).toHaveBeenCalled());
    expect(order).toEqual(['call', 'instance']);
  });

  it('Return goes to the channel without joining the instance again', async () => {
    const user = userEvent.setup();
    const api = lobbyApi(() =>
      Promise.resolve({ enabled: true, items: [lobby({ joined: true })] }),
    );
    page(api);
    await user.click(await screen.findByRole('button', { name: 'Return to Land.io in general' }));
    await waitFor(() => expect(navigate).toHaveBeenCalled());
    expect(api.join).not.toHaveBeenCalled();
  });

  it('says a game ended on a 404 and fetches the strip again', async () => {
    const user = userEvent.setup();
    const api = lobbyApi(() => Promise.resolve({ enabled: true, items: [lobby()] }), {
      join: vi.fn(() => Promise.reject(new ApiError(404, 'Unknown activity'))),
    });
    page(api);
    await user.click(await screen.findByRole('button', { name: 'Join Land.io in general' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('This game has ended.');
    expect(navigate).not.toHaveBeenCalled();
    const before = vi.mocked(api.lobbies).mock.calls.length;
    await waitFor(() => expect(vi.mocked(api.lobbies).mock.calls.length).toBeGreaterThan(before), {
      timeout: 4000,
    });
  });

  it('refetches once within a second of instance events', async () => {
    const api = lobbyApi(() => Promise.resolve({ enabled: true, items: [] }));
    page(api);
    await waitFor(() => expect(api.lobbies).toHaveBeenCalledTimes(1));
    vi.useFakeTimers();
    act(() => {
      emitDispatch('SR_ACTIVITY_INSTANCE_UPDATE', { channel_id: '1', instance: null });
      emitDispatch('SR_ACTIVITY_INSTANCE_UPDATE', { channel_id: '2', instance: null });
      emitDispatch('MESSAGE_CREATE', {});
    });
    expect(api.lobbies).toHaveBeenCalledTimes(1);
    await act(async () => {
      vi.advanceTimersByTime(1000);
    });
    expect(api.lobbies).toHaveBeenCalledTimes(2);
  });

  it('puts an “N live” chip on a built-in card with lobbies, linking to the strip (D-487)', async () => {
    const api = lobbyApi(() =>
      Promise.resolve({
        enabled: true,
        items: [
          lobby(),
          lobby({ instance_id: '502' }),
          lobby({ instance_id: '503', application_name: 'Other' }),
        ],
      }),
    );
    page(api);
    const playable = await screen.findByRole('region', { name: 'Playable now' });
    const chip = await within(playable).findByRole('link', { name: '2 lobbies running now' });
    expect(chip).toHaveAttribute('href', '#lobbies');
    expect(chip).toHaveTextContent('2 live');
  });

  it('puts the chip on the hero card of a game with lobbies too (D-500)', async () => {
    const api = lobbyApi(() =>
      Promise.resolve({ enabled: true, items: [lobby(), lobby({ instance_id: '502' })] }),
    );
    const { container } = render(
      <ArcadePage api={arcadeApi} trust={trust} lobbyApi={api} callJoin={vi.fn()} />,
    );
    await screen.findByRole('heading', { name: 'Active lobbies' });
    const hero = container.querySelector('.arc-hero-cards') as HTMLElement;
    const land = within(hero).getByRole('article', { name: 'Land.io' });
    const chip = within(land).getByRole('link', { name: '2 lobbies running now' });
    expect(chip).toHaveAttribute('href', '#lobbies');
    // Arena 3D, the other hero, has no lobby and no chip.
    const arena = within(hero).getByRole('article', { name: 'Arena 3D' });
    expect(within(arena).queryByRole('link', { name: /running now/ })).toBeNull();
  });

  it('says a game could not be joined on a 403 and fetches the strip again after the delay (D-498)', async () => {
    const user = userEvent.setup();
    const api = lobbyApi(() => Promise.resolve({ enabled: true, items: [lobby()] }), {
      join: vi.fn(() => Promise.reject(new ApiError(403, 'Missing Permissions'))),
    });
    page(api);
    await user.click(await screen.findByRole('button', { name: 'Join Land.io in general' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('This game could not be joined.');
    const before = vi.mocked(api.lobbies).mock.calls.length;
    // Not at once: the line is read first.
    await new Promise((r) => setTimeout(r, 1000));
    expect(vi.mocked(api.lobbies).mock.calls.length).toBe(before);
    expect(screen.getByRole('alert')).toBeInTheDocument();
    await waitFor(() => expect(vi.mocked(api.lobbies).mock.calls.length).toBeGreaterThan(before), {
      timeout: 4000,
    });
    expect(navigate).not.toHaveBeenCalled();
  });

  it('ticks a lobby card’s age on a one-minute clock, with no request (D-501)', () => {
    vi.useFakeTimers();
    const start = Date.now();
    const onJoin = vi.fn();
    render(
      <LobbiesSection
        items={[lobby({ created_at_ms: start - 30_000 })]}
        onJoin={onJoin}
        now={start}
      />,
    );
    expect(screen.getByText('just started')).toBeInTheDocument();
    act(() => {
      vi.advanceTimersByTime(LOBBY_CLOCK_MS);
    });
    expect(screen.getByText('for 1 min')).toBeInTheDocument();
  });

  it('counts lobbies by name for a built-in and by id for a pick', () => {
    const items = [
      lobby(),
      lobby({ application_id: '7', application_name: 'Orbit' }),
      lobby({ application_id: '7', application_name: 'Orbit' }),
    ];
    expect(liveCount(items, { name: 'Land.io' })).toBe(1);
    expect(liveCount(items, { applicationId: '7' })).toBe(2);
    expect(liveCount(items, { name: 'Nothing' })).toBe(0);
  });
});

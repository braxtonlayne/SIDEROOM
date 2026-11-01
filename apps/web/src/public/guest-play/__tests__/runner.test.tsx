import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { createMemoryHistory, RouterProvider } from '@tanstack/react-router';
import { ACTIVITY_SANDBOX } from '../../../activities/ActivityFrame';
import { createPublicRouter } from '../../../test/publicRouter';
import { GUEST_SANDBOX, Runner, gameFrameUrl } from '../Runner';
import { generateGameKey } from '../link';
import type { GuestJoined } from '../contract';
import type { Lobby } from '../session';

const TOKEN = 'ef56'.repeat(16);
const KEY = generateGameKey();
const where = { pathname: `/play/${TOKEN}`, hash: `#key=${KEY}`, protocol: 'https:', host: 'chat.example.org' };

const joined = (over: Partial<GuestJoined> = {}): GuestJoined => ({
  guest_token: 'srg_' + 'b'.repeat(64),
  callsign: 'Amber Otter 42',
  expires_at: '2026-11-01T12:00:00Z',
  topic: `game:${TOKEN}`,
  socket_path: '/gateway/guest',
  application_id: '777',
  application_name: 'Land.io',
  ...over,
});

function fakeLobby(): Lobby {
  return {
    seated: Promise.resolve({
      seat: 2,
      role: 'guest',
      callsign: 'Amber Otter 42',
      peers: [
        { seat: 2, role: 'guest', callsign: 'Amber Otter 42' },
        { seat: 1, role: 'member', callsign: 'Host' },
      ],
    }),
    peers: () => [
      { seat: 2, role: 'guest', callsign: 'Amber Otter 42' },
      { seat: 1, role: 'member', callsign: 'Host' },
    ],
    onPeers: () => () => undefined,
    publish: async () => true,
    onMessage: () => () => undefined,
    onEnd: () => () => undefined,
    close: vi.fn(),
  };
}

describe('the guest runner page', () => {
  it('shares the member frame’s sandbox string (scripts and pointer lock, never same-origin)', () => {
    expect(GUEST_SANDBOX).toBe(ACTIVITY_SANDBOX);
    expect(GUEST_SANDBOX).not.toContain('allow-same-origin');
  });

  it('addresses the game on its own activity origin and refuses an application id that is not a number', () => {
    expect(gameFrameUrl('777', where)).toBe('https://777.activities.chat.example.org/');
    expect(gameFrameUrl('7.evil.test/', where)).toBeNull();
    expect(gameFrameUrl('', where)).toBeNull();
  });

  it('says so, and calls nothing, for a link with no valid token', () => {
    const join = vi.fn();
    render(<Runner loc={{ ...where, pathname: '/play/short' }} join={join} />);
    expect(screen.getByRole('alert')).toHaveTextContent('This link is not valid');
    expect(join).not.toHaveBeenCalled();
  });

  it('says so, and calls nothing, for a link whose key fragment is missing', () => {
    const join = vi.fn();
    render(<Runner loc={{ ...where, hash: '' }} join={join} />);
    expect(screen.getByRole('alert')).toHaveTextContent('incomplete');
    expect(join).not.toHaveBeenCalled();
  });

  it('joins on a click, opens the lobby socket for the guest and runs the game in the sandbox', async () => {
    const lobby = fakeLobby();
    const join = vi.fn(async () => joined({ capabilities: ['guest_play', 'datachannel'] }));
    const open = vi.fn(() => lobby);
    render(<Runner loc={where} join={join} open={open} hostWaitMs={0} />);
    expect(join).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole('button', { name: 'Join the game' }));
    const frame = await screen.findByTitle('Game: Land.io');
    expect(frame).toHaveAttribute('src', 'https://777.activities.chat.example.org/');
    expect(frame).toHaveAttribute('sandbox', 'allow-scripts allow-pointer-lock');
    expect(frame).toHaveAttribute('referrerpolicy', 'no-referrer');
    expect(join).toHaveBeenCalledWith(TOKEN);
    const o = (open.mock.calls[0] as unknown as [Record<string, unknown>])[0];
    expect(o.hello).toEqual({ guest_token: 'srg_' + 'b'.repeat(64) });
    expect(String(o.url)).toBe('wss://chat.example.org/gateway/guest?v=1');
    expect(screen.getByText(/Playing as Amber Otter 42/)).toBeInTheDocument();
  });

  it('refuses a game that declares ingame_voice before it opens any socket', async () => {
    const open = vi.fn();
    const join = vi.fn(async () => joined({ capabilities: ['guest_play', 'datachannel', 'ingame_voice'] }));
    render(<Runner loc={where} join={join} open={open} />);
    await userEvent.click(screen.getByRole('button', { name: 'Join the game' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('in-game voice');
    expect(open).not.toHaveBeenCalled();
    expect(screen.queryByTitle(/Game:/)).toBeNull();
  });

  it('refuses a game that is not open to guests', async () => {
    const open = vi.fn();
    const join = vi.fn(async () => joined({ capabilities: ['datachannel'] }));
    render(<Runner loc={where} join={join} open={open} />);
    await userEvent.click(screen.getByRole('button', { name: 'Join the game' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('not open to guests');
    expect(open).not.toHaveBeenCalled();
  });

  it('explains an expired link and a rate limit, and offers a retry only where one can help', async () => {
    const { JoinError } = await import('../session');
    const join = vi
      .fn()
      .mockRejectedValueOnce(new JoinError('gone'))
      .mockRejectedValueOnce(new JoinError('rate_limited'));
    const view = render(<Runner loc={where} join={join} />);
    await userEvent.click(screen.getByRole('button', { name: 'Join the game' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('expired');
    expect(screen.queryByRole('button', { name: 'Try again' })).toBeNull();
    view.unmount();
    render(<Runner loc={where} join={join} />);
    await userEvent.click(screen.getByRole('button', { name: 'Join the game' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Too many tries');
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });

  it('makes only the join call: nothing else leaves the page before the socket', async () => {
    const f = vi.spyOn(globalThis, 'fetch');
    const open = vi.fn(() => fakeLobby());
    const join = vi.fn(async () => joined({ capabilities: ['guest_play'] }));
    render(<Runner loc={where} join={join} open={open} hostWaitMs={0} />);
    await userEvent.click(screen.getByRole('button', { name: 'Join the game' }));
    await screen.findByTitle('Game: Land.io');
    expect(f).not.toHaveBeenCalled();
    f.mockRestore();
  });
});

describe('the surface route', () => {
  it('serves /play/<token> on the guest-play surface and shows the join screen', async () => {
    const path = `/play/${TOKEN}#key=${KEY}`;
    const router = createPublicRouter(`/play/${TOKEN}`, createMemoryHistory({ initialEntries: [path] }));
    render(<RouterProvider router={router} />);
    expect(await screen.findByRole('button', { name: 'Join the game' })).toBeInTheDocument();
  });

  it('marks the page noindex', async () => {
    const router = createPublicRouter(`/play/${TOKEN}`, createMemoryHistory({ initialEntries: [`/play/${TOKEN}#key=${KEY}`] }));
    render(<RouterProvider router={router} />);
    await screen.findByRole('button', { name: 'Join the game' });
    await waitFor(() =>
      expect(document.head.querySelector('meta[name="robots"][content="noindex"]')).not.toBeNull(),
    );
  });

  it('shows the invalid-link page for a short token and nothing of the app', async () => {
    const router = createPublicRouter('/play/x', createMemoryHistory({ initialEntries: ['/play/x'] }));
    render(<RouterProvider router={router} />);
    expect(await screen.findByRole('alert')).toHaveTextContent('not valid');
    expect(document.body.textContent).not.toMatch(/lounge|cinema|library|channel/i);
  });
});

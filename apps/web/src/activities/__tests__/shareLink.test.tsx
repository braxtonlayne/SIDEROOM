// ARC-GST-002: the Share link dialog and its button in the Activity panel.
import { ThemeProvider } from '@sideroom/ui';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { GAME_KEY } from '../../public/guest-play/contract';
import type { GuestLinkApi } from '../guestLinks';
import { loadLink } from '../guestLinks';
import { canShare, ShareLinkDialog } from '../ShareLinkDialog';
import type { ActivityInstance } from '../types';

const startBridge = vi.fn();
const stopBridge = vi.fn();
vi.mock('../guestBridge', () => ({
  startBridge: (o: unknown) => startBridge(o),
  stopBridge: (i: string) => stopBridge(i),
  guestAwareData: (r: unknown) => r,
}));

const TOKEN = 'cd34'.repeat(16);
const inst = (over: Partial<ActivityInstance> = {}): ActivityInstance => ({
  id: '9',
  application_id: '7',
  application_name: 'Land.io',
  guild_id: '2',
  channel_id: '3',
  channel_type: 'voice',
  development: false,
  started_by: '10',
  created_at_ms: 0,
  participants: [{ user_id: '10', joined_at_ms: 1 }],
  capabilities: ['datachannel', 'guest_play', 'lobby_channel'],
  ...over,
});

const api = (over: Partial<GuestLinkApi> = {}): GuestLinkApi => ({
  create: vi.fn(async () => ({
    lobby_token: TOKEN,
    expires_at: new Date(Date.now() + 3_600_000).toISOString(),
  })),
  revoke: vi.fn(async () => undefined),
  ...over,
});

const show = (a: GuestLinkApi, i = inst()) =>
  render(
    <ThemeProvider section="lounge">
      <ShareLinkDialog instance={i} open onClose={() => undefined} api={a} origin="https://chat.example.org" />
    </ThemeProvider>,
  );

beforeEach(() => {
  sessionStorage.clear();
  startBridge.mockClear();
  stopBridge.mockClear();
});
afterEach(() => vi.unstubAllGlobals());

describe('who can share', () => {
  it('needs a voice-channel lobby and a game that declares guest_play', () => {
    expect(canShare(inst())).toBe(true);
    expect(canShare(inst({ capabilities: ['datachannel'] }))).toBe(false);
    expect(canShare(inst({ capabilities: undefined }))).toBe(false);
    expect(canShare(inst({ channel_type: 'text' }))).toBe(false);
  });

  it('is closed to a game that declares ingame_voice (the runner would refuse it)', () => {
    expect(canShare(inst({ capabilities: ['guest_play', 'ingame_voice'] }))).toBe(false);
  });
});

describe('the Share link dialog', () => {
  it('makes the key here, asks the server for the token only, and shows the link with a QR code', async () => {
    const a = api();
    show(a);
    await userEvent.click(screen.getByRole('button', { name: 'Create link' }));
    const field = (await screen.findByLabelText('Guest link')) as HTMLInputElement;
    const m = /^https:\/\/chat\.example\.org\/play\/([0-9a-f]{64})#key=([A-Za-z0-9_-]{43})$/.exec(field.value);
    expect(m).not.toBeNull();
    expect(m![1]).toBe(TOKEN);
    expect(m![2]).toMatch(GAME_KEY);
    // The server call carried no key: it was told the instance and nothing else.
    expect(a.create).toHaveBeenCalledWith('9');
    expect(JSON.stringify((a.create as ReturnType<typeof vi.fn>).mock.calls)).not.toContain(m![2]);
    expect(screen.getByRole('img', { name: 'QR code for the guest link' })).toBeInTheDocument();
    expect(startBridge).toHaveBeenCalledWith(
      expect.objectContaining({ instanceId: '9', lobbyToken: TOKEN, gameKey: m![2], hostId: '10' }),
    );
  });

  it('remembers the key in this tab only (session storage), and shows the same link again', async () => {
    const a = api();
    const view = show(a);
    await userEvent.click(screen.getByRole('button', { name: 'Create link' }));
    const first = ((await screen.findByLabelText('Guest link')) as HTMLInputElement).value;
    expect(loadLink('9')?.lobbyToken).toBe(TOKEN);
    expect(localStorage.length).toBe(0);
    view.unmount();
    show(a);
    expect(((await screen.findByLabelText('Guest link')) as HTMLInputElement).value).toBe(first);
    expect(screen.queryByRole('button', { name: 'Create link' })).toBeNull();
  });

  it('copies the whole link', async () => {
    const writeText = vi.fn(async () => undefined);
    vi.stubGlobal('navigator', { ...navigator, clipboard: { writeText } });
    show(api());
    await userEvent.click(screen.getByRole('button', { name: 'Create link' }));
    const href = ((await screen.findByLabelText('Guest link')) as HTMLInputElement).value;
    await userEvent.click(screen.getByRole('button', { name: 'Copy link' }));
    expect(writeText).toHaveBeenCalledWith(href);
    expect(await screen.findByRole('button', { name: 'Link copied' })).toBeInTheDocument();
  });

  it('revokes: calls the server, stops the bridge and forgets the key', async () => {
    const a = api();
    show(a);
    await userEvent.click(screen.getByRole('button', { name: 'Create link' }));
    await screen.findByLabelText('Guest link');
    await userEvent.click(screen.getByRole('button', { name: 'Revoke link' }));
    await waitFor(() => expect(a.revoke).toHaveBeenCalledWith('9', TOKEN));
    expect(stopBridge).toHaveBeenCalledWith('9');
    expect(loadLink('9')).toBeNull();
    expect(await screen.findByText('The link no longer works.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Create link' })).toBeInTheDocument();
  });

  it('says so when the link could not be made, and when it could not be revoked', async () => {
    const bad = api({
      create: vi.fn(async () => {
        throw new Error('403');
      }),
    });
    const view = show(bad);
    await userEvent.click(screen.getByRole('button', { name: 'Create link' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('could not be created');
    expect(startBridge).not.toHaveBeenCalled();
    view.unmount();

    const stuck = api({
      revoke: vi.fn(async () => {
        throw new Error('500');
      }),
    });
    show(stuck);
    await userEvent.click(screen.getByRole('button', { name: 'Create link' }));
    await screen.findByLabelText('Guest link');
    await userEvent.click(screen.getByRole('button', { name: 'Revoke link' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('could not be revoked');
    expect(stopBridge).not.toHaveBeenCalled();
    expect(loadLink('9')).not.toBeNull();
  });

  it('forgets an expired stored link', () => {
    sessionStorage.setItem(
      'sr.guestlink.9',
      JSON.stringify({ lobbyToken: TOKEN, gameKey: 'k'.repeat(43), expiresAt: '2020-01-01T00:00:00Z' }),
    );
    expect(loadLink('9')).toBeNull();
    expect(sessionStorage.getItem('sr.guestlink.9')).toBeNull();
  });
});

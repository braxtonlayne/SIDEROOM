import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterAll, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../../test/server';
import {
  applyStatusEvent,
  resetChannelStatus,
  useChannelStatus,
} from '../../../voice/channelStatus';
import type { Channel } from '../../data';
import { MockDataProvider } from '../../mockData';
import { ChannelStatusLine, VoiceStatusBar } from '../VoiceStatusBar';

beforeAll(() => vi.stubEnv('VITE_SR_MOCK', '1'));
afterAll(() => vi.unstubAllEnvs());

const perms = {
  connect: true,
  speak: true,
  useVad: true,
  muteMembers: false,
  deafenMembers: false,
  moveMembers: false,
  useSoundboard: true,
  requestToSpeak: true,
  setVoiceStatus: true,
  manageChannels: false,
};
vi.mock('../perms', () => ({ useVoicePerms: () => perms }));

const channel = {
  id: '2011',
  guildId: '1001',
  type: 'voice',
  name: 'General',
  position: 0,
} as Channel;
const puts: { url: string; body: unknown }[] = [];

beforeEach(() => {
  puts.length = 0;
  resetChannelStatus();
  Object.assign(perms, { setVoiceStatus: true, manageChannels: false });
  server.use(
    http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 'csrf' })),
    http.put('*/api/v1/channels/2011/voice-status', async ({ request }) => {
      puts.push({ url: new URL(request.url).pathname, body: await request.json() });
      return new HttpResponse(null, { status: 204 });
    }),
  );
});

describe('channel status events', () => {
  it('follows VOICE_CHANNEL_STATUS_UPDATE and GUILD_CREATE', () => {
    applyStatusEvent('VOICE_CHANNEL_STATUS_UPDATE', { id: '5', guild_id: '1', status: 'Karaoke' });
    expect(useChannelStatus.getState().byChannel).toEqual({ '5': 'Karaoke' });
    applyStatusEvent('VOICE_CHANNEL_STATUS_UPDATE', { id: '5', guild_id: '1', status: null });
    expect(useChannelStatus.getState().byChannel).toEqual({});
    // GUILD_CREATE replaces what we knew about that server's channels, and leaves the others.
    applyStatusEvent('VOICE_CHANNEL_STATUS_UPDATE', { id: '5', status: 'old' });
    applyStatusEvent('VOICE_CHANNEL_STATUS_UPDATE', { id: '99', status: 'other server' });
    applyStatusEvent('GUILD_CREATE', {
      id: '1',
      channels: [{ id: '5' }, { id: '6', status: 'Study hall' }, { name: 'no id' }],
    });
    expect(useChannelStatus.getState().byChannel).toEqual({
      '6': 'Study hall',
      '99': 'other server',
    });
    applyStatusEvent('MESSAGE_CREATE', { id: '5', status: 'x' });
    applyStatusEvent('VOICE_CHANNEL_STATUS_UPDATE', { status: 'no id' });
    expect(Object.keys(useChannelStatus.getState().byChannel)).toHaveLength(2);
  });
});

describe('voice channel status (VOX-UI-015)', () => {
  const show = (here = true) => {
    const user = userEvent.setup();
    render(
      <MockDataProvider>
        <VoiceStatusBar channel={channel} here={here} />
      </MockDataProvider>,
    );
    return user;
  };

  it('shows the status and a pencil to someone in the channel, and saves a new one', async () => {
    act(() =>
      applyStatusEvent('VOICE_CHANNEL_STATUS_UPDATE', { id: '2011', status: 'Mario Kart' }),
    );
    const user = show();
    expect(screen.getByTestId('voice-status')).toHaveTextContent('Mario Kart');
    await user.click(screen.getByRole('button', { name: 'Set channel status' }));
    const dialog = await screen.findByRole('dialog', { name: 'Channel status' });
    const box = within(dialog).getByRole('textbox', { name: 'Status' });
    expect(box).toHaveValue('Mario Kart');
    await user.clear(box);
    await user.type(box, '  Chess night ');
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(puts).toHaveLength(1));
    expect(puts[0]).toEqual({
      url: '/api/v1/channels/2011/voice-status',
      body: { status: 'Chess night' },
    });
  });

  it('clears the status', async () => {
    act(() =>
      applyStatusEvent('VOICE_CHANNEL_STATUS_UPDATE', { id: '2011', status: 'Mario Kart' }),
    );
    const user = show();
    await user.click(screen.getByRole('button', { name: 'Set channel status' }));
    const dialog = await screen.findByRole('dialog', { name: 'Channel status' });
    await user.click(within(dialog).getByRole('button', { name: 'Clear status' }));
    await waitFor(() => expect(puts).toHaveLength(1));
    expect(puts[0]!.body).toEqual({ status: null });
  });

  it('saving an empty text clears it, and there is no Clear button when nothing is set', async () => {
    const user = show();
    expect(screen.getByTestId('voice-status')).toHaveTextContent('No status');
    await user.click(screen.getByRole('button', { name: 'Set channel status' }));
    const dialog = await screen.findByRole('dialog', { name: 'Channel status' });
    expect(within(dialog).queryByRole('button', { name: 'Clear status' })).toBeNull();
    await user.type(within(dialog).getByRole('textbox', { name: 'Status' }), '   ');
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(puts).toHaveLength(1));
    expect(puts[0]!.body).toEqual({ status: null });
  });

  it('hides the pencil from someone outside the channel or without the permission, but shows the status', () => {
    act(() =>
      applyStatusEvent('VOICE_CHANNEL_STATUS_UPDATE', { id: '2011', status: 'Mario Kart' }),
    );
    const { unmount } = render(
      <MockDataProvider>
        <VoiceStatusBar channel={channel} here={false} />
      </MockDataProvider>,
    );
    expect(screen.getByTestId('voice-status')).toHaveTextContent('Mario Kart');
    expect(screen.queryByRole('button', { name: 'Set channel status' })).toBeNull();
    unmount();
    perms.setVoiceStatus = false;
    show(true);
    expect(screen.queryByRole('button', { name: 'Set channel status' })).toBeNull();
  });

  it('shows nothing at all when there is no status and nobody here may set one', () => {
    perms.setVoiceStatus = false;
    show(true);
    expect(screen.queryByTestId('voice-status')).toBeNull();
  });

  it('lets a manager set it from outside the channel', async () => {
    perms.manageChannels = true;
    perms.setVoiceStatus = false;
    const user = show(false);
    await user.click(screen.getByRole('button', { name: 'Set channel status' }));
    expect(await screen.findByRole('dialog', { name: 'Channel status' })).toBeInTheDocument();
  });

  it('says so when the server refuses', async () => {
    server.use(
      http.put('*/api/v1/channels/2011/voice-status', () =>
        HttpResponse.json({ code: 50013, message: 'Missing Permissions' }, { status: 403 }),
      ),
    );
    const user = show();
    await user.click(screen.getByRole('button', { name: 'Set channel status' }));
    const dialog = await screen.findByRole('dialog', { name: 'Channel status' });
    await user.type(within(dialog).getByRole('textbox', { name: 'Status' }), 'x');
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('cannot set the status');
  });

  it('has no a11y violations', async () => {
    act(() =>
      applyStatusEvent('VOICE_CHANNEL_STATUS_UPDATE', { id: '2011', status: 'Mario Kart' }),
    );
    show();
    const bar = screen.getByTestId('voice-status').parentElement!;
    expect(await axeViolations(bar)).toEqual([]);
  });
});

describe('sidebar status line', () => {
  it('shows the status under the channel and nothing without one', () => {
    const { rerender } = render(<ChannelStatusLine channelId="2011" />);
    expect(screen.queryByTestId('channel-status-2011')).toBeNull();
    act(() => applyStatusEvent('VOICE_CHANNEL_STATUS_UPDATE', { id: '2011', status: 'Karaoke' }));
    rerender(<ChannelStatusLine channelId="2011" />);
    expect(screen.getByTestId('channel-status-2011')).toHaveTextContent('Karaoke');
  });
});

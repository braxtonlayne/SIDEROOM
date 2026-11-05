// D-472 (ARC-UI-011, built with the ARC-UI-010 rework): Play opens "Start in…": the member's call first, then the
// server's voice channels and the text channels; the pick opens the ActivityLauncher with the game preselected, which
// launches at once, and the member is taken to that channel. A refused launch falls back to the shelf.
import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import type { ReactNode } from 'react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '../../api/client';
import type { ActivityApi } from '../../activities/api';
import type { Channel } from '../../store/types';
import { IDLE_SESSION, useVoiceSession } from '../../voice/session';
import { startableChannels, StartInDialog } from '../StartInDialog';

const navigate = vi.fn();
vi.mock('@tanstack/react-router', () => ({
  useNavigate: () => navigate,
  Link: ({ to, children }: { to: string; children: ReactNode }) => <a href={to}>{children}</a>,
}));

const channels: Channel[] = [
  { id: 't1', type: 0, guild_id: '1', name: 'general', position: 0 },
  { id: 'c1', type: 4, guild_id: '1', name: 'Category', position: 0 },
  { id: 'v1', type: 2, guild_id: '1', name: 'Hangout', position: 1 },
  { id: 'a1', type: 5, guild_id: '1', name: 'news', position: 2 },
];

function activityApi(over: Partial<ActivityApi> = {}): ActivityApi {
  return {
    shelf: vi.fn(() =>
      Promise.resolve({
        enabled: true,
        items: [{ application_id: '42', name: 'Land.io', description: '', development: false }],
      }),
    ),
    inChannel: vi.fn(() => Promise.resolve({ enabled: true, instance: null })),
    launch: vi.fn((channel_id: string, application_id: string) =>
      Promise.resolve({
        id: '9',
        application_id,
        application_name: 'Land.io',
        guild_id: '1',
        channel_id,
        channel_type: 'voice' as const,
        development: false,
        started_by: '1',
        created_at_ms: 1,
        participants: [],
      }),
    ),
    join: vi.fn(),
    leave: vi.fn(),
    end: vi.fn(),
    ...over,
  };
}

afterEach(() => {
  act(() => useVoiceSession.setState(IDLE_SESSION));
  navigate.mockReset();
});

describe('Start in…', () => {
  it('offers voice channels first, then text and announcement channels, never categories', () => {
    expect(startableChannels(channels).map((c) => c.id)).toEqual(['v1', 't1', 'a1']);
  });

  it('puts the member’s call first, launches the preselected game there and goes to it', async () => {
    const user = userEvent.setup();
    act(() =>
      useVoiceSession.setState({
        ...IDLE_SESSION,
        status: 'connected',
        guildId: '1',
        channelId: 'v1',
        channelName: 'Hangout',
      }),
    );
    const api = activityApi();
    const onClose = vi.fn();
    render(
      <StartInDialog
        target={{ title: 'Land.io', shelfName: 'Land.io' }}
        guilds={[{ id: '1', name: 'Friends' }]}
        onClose={onClose}
        api={api}
        loadChannels={() => Promise.resolve(channels)}
      />,
    );
    const dialog = screen.getByRole('dialog', { name: 'Start Land.io in…' });
    const call = within(dialog).getByRole('region', { name: 'Your call' });
    await user.click(within(call).getByRole('button', { name: 'Start Land.io in Hangout' }));
    await waitFor(() => expect(api.launch).toHaveBeenCalledWith('v1', '42'));
    await waitFor(() =>
      expect(navigate).toHaveBeenCalledWith({
        to: '/channels/$guildId/$channelId',
        params: { guildId: '1', channelId: 'v1' },
      }),
    );
    expect(onClose).toHaveBeenCalled();
  });

  it('lists the server’s channels when the member is not in a call, and falls back to the shelf when refused', async () => {
    const user = userEvent.setup();
    const api = activityApi({
      launch: vi.fn(() => Promise.reject(new ApiError(403, 'no'))),
    });
    render(
      <StartInDialog
        target={{ title: 'Land.io', shelfName: 'Land.io' }}
        guilds={[{ id: '1', name: 'Friends' }]}
        onClose={vi.fn()}
        api={api}
        loadChannels={() => Promise.resolve(channels)}
      />,
    );
    expect(screen.queryByRole('region', { name: 'Your call' })).toBeNull();
    const rows = await screen.findAllByRole('button', { name: /^Start Land\.io in / });
    expect(rows.map((r) => r.getAttribute('aria-label'))).toEqual([
      'Start Land.io in Hangout',
      'Start Land.io in general',
      'Start Land.io in news',
    ]);
    await user.click(rows[1]!);
    await waitFor(() => expect(api.launch).toHaveBeenCalledWith('t1', '42'));
    // The launcher stays open with its shelf and the reason; nothing navigates.
    expect(await screen.findByRole('button', { name: 'Start Land.io' })).toBeInTheDocument();
    expect(navigate).not.toHaveBeenCalled();
  });

  it('uses a Featured pick’s own application id without asking the shelf', async () => {
    const user = userEvent.setup();
    const api = activityApi();
    render(
      <StartInDialog
        target={{ title: 'Orbit', applicationId: '77' }}
        guilds={[{ id: '1', name: 'Friends' }]}
        onClose={vi.fn()}
        api={api}
        loadChannels={() => Promise.resolve(channels)}
      />,
    );
    await user.click(await screen.findByRole('button', { name: 'Start Orbit in Hangout' }));
    await waitFor(() => expect(api.launch).toHaveBeenCalledWith('v1', '77'));
  });
});

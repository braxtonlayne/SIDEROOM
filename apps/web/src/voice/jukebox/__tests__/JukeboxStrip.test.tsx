import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { JukeboxStrip } from '../JukeboxStrip';
import type { JukeboxApi, JukeboxState } from '../types';

const base: JukeboxState = {
  channel_id: '9',
  active: true,
  mode: 'synced',
  version: 3,
  server_now_ms: Date.now(),
  position_ms: 1_000,
  started_at_ms: Date.now() - 1_000,
  paused: false,
  current: { id: '1', state: 'playing', playable: false, duration_ms: 60_000 },
  queue: [
    { id: '2', state: 'queued', playable: true, title: 'Next one', artist: 'Band' },
    { id: '3', state: 'queued', playable: false },
  ],
  votes: 1,
  votes_needed: 2,
  my_vote: false,
  listeners: 2,
  i_am_dj: false,
  can_queue: true,
  can_control: true,
};

function api(): JukeboxApi {
  return {
    listen: vi.fn(async () => base),
    state: vi.fn(async () => base),
    voteSkip: vi.fn(async () => ({ ...base, my_vote: true, votes: 2 })),
    leave: vi.fn(async () => base),
    library: vi.fn(async () => ({ items: [], total: 0 })),
  } as unknown as JukeboxApi;
}

HTMLMediaElement.prototype.pause = () => undefined;
HTMLMediaElement.prototype.play = () => Promise.resolve();

describe('JukeboxStrip', () => {
  it('offers to listen first, then shows the queue with not-shared rows, with no axe violations', async () => {
    const a = api();
    const { container } = render(<JukeboxStrip channelId="9" meId="1" api={a} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Listen along' }));
    expect((await screen.findByTestId('jukebox-now')).textContent).toBe('Not shared with you');
    expect(screen.getByText('Next one – Band')).toBeTruthy();
    expect(screen.getByRole('button', { name: /Vote to skip \(1\/2\)/ })).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });

  it('votes to skip through the api', async () => {
    const a = api();
    render(<JukeboxStrip channelId="9" meId="1" api={a} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Listen along' }));
    await userEvent.click(await screen.findByRole('button', { name: /Vote to skip/ }));
    expect(a.voteSkip).toHaveBeenCalledWith('9');
  });
});

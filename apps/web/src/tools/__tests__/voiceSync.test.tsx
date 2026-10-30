import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useVoiceSession } from '../../voice/session';
import PomodoroTool from '../timers/PomodoroTool';
import { resetSharedTimers } from '../timers/timerStore';
import { applyVoiceTimerEvent, resetVoiceTimers } from '../timers/voiceTimer';

const publish = vi.hoisted(() =>
  vi.fn(
    async (_channel: string, _wire: Record<string, unknown> | null, _label: string) => undefined,
  ),
);
vi.mock('../timers/voiceTimer', async (orig) => ({
  ...(await orig<typeof import('../timers/voiceTimer')>()),
  publishVoiceTimer: publish,
}));
const perms = vi.hoisted(() => ({ setVoiceStatus: true, manageChannels: false }));
vi.mock('../../lounge/voice/perms', () => ({ useVoicePerms: () => perms }));

const props = { host: 'drawer' as const };

beforeEach(() => {
  vi.useFakeTimers({
    toFake: ['setInterval', 'clearInterval', 'setTimeout', 'clearTimeout', 'Date'],
  });
  vi.setSystemTime(new Date('2030-01-01T00:00:00Z'));
  publish.mockClear();
  perms.setVoiceStatus = true;
  useVoiceSession.setState({ channelId: '77', guildId: '5' });
});
afterEach(() => {
  vi.useRealTimers();
  resetSharedTimers();
  resetVoiceTimers();
  useVoiceSession.setState({ channelId: null, guildId: null });
  try {
    localStorage.clear();
  } catch {
    /* fine */
  }
});

describe('sharing a timer with the voice channel', () => {
  it('cannot share a timer that is not running, then shares, follows pauses and stops on reset', async () => {
    render(<PomodoroTool {...props} />);
    const share = screen.getByRole('button', { name: 'Share with my voice channel' });
    expect(share.hasAttribute('disabled')).toBe(true);

    fireEvent.click(screen.getByRole('button', { name: 'Start' }));
    await act(async () => {
      vi.advanceTimersByTime(1000);
    });
    fireEvent.click(share);
    await act(async () => {});
    expect(publish).toHaveBeenCalledTimes(1);
    const [channel, wire, label] = publish.mock.calls[0] as unknown as [
      string,
      Record<string, unknown>,
      string,
    ];
    expect(channel).toBe('77');
    expect(wire).toMatchObject({
      kind: 'pomodoro',
      phase: 'running',
      segment: 'focus',
      durationMs: 1_500_000,
    });
    expect(label).toBe('Focus 24:59');
    expect(screen.getByRole('button', { name: 'Stop sharing' }).getAttribute('aria-pressed')).toBe(
      'true',
    );

    // A pause is sent on, with the new state.
    fireEvent.click(screen.getByRole('button', { name: 'Pause' }));
    await act(async () => {});
    expect(publish).toHaveBeenCalledTimes(2);
    expect(publish.mock.calls[1]![1]).toMatchObject({ phase: 'paused', startedMs: null });

    // A reset ends the sharing.
    fireEvent.click(screen.getByRole('button', { name: 'Reset' }));
    await act(async () => {});
    expect(publish).toHaveBeenCalledTimes(3);
    expect(publish.mock.calls[2]![1]).toBeNull();
    expect(screen.getByRole('button', { name: 'Share with my voice channel' })).toBeTruthy();
  });

  it('says what is missing without the permission, and shows an error when the server refuses', async () => {
    perms.setVoiceStatus = false;
    const { unmount } = render(<PomodoroTool {...props} />);
    expect(screen.getByText(/permission to set the channel status/)).toBeTruthy();
    expect(
      screen.getByRole('button', { name: 'Share with my voice channel' }).hasAttribute('disabled'),
    ).toBe(true);
    unmount();

    perms.setVoiceStatus = true;
    publish.mockRejectedValueOnce(new Error('boom'));
    render(<PomodoroTool {...props} />);
    fireEvent.click(screen.getByRole('button', { name: 'Start' }));
    fireEvent.click(screen.getByRole('button', { name: 'Share with my voice channel' }));
    await act(async () => {});
    expect(screen.getByRole('alert').textContent).toBe('Could not share the timer.');
  });

  it("shows another member's shared timer on the server clock", async () => {
    render(<PomodoroTool {...props} />);
    // The server's clock is 10 s ahead of this browser's; the timer started 60 s ago on the server's clock.
    const now = Date.now();
    applyVoiceTimerEvent(
      'VOICE_TIMER_UPDATE',
      {
        id: '77',
        user_id: '9',
        server_ms: now + 10_000,
        timer: {
          kind: 'pomodoro',
          phase: 'running',
          segment: 'focus',
          startedMs: now + 10_000 - 60_000,
          accumulatedMs: 0,
          durationMs: 1_500_000,
          completed: 0,
        },
      },
      now,
    );
    await act(async () => {});
    expect(screen.getByText(/shared by someone in this channel/)).toBeTruthy();
    expect(screen.getByText(/Focus 24:00, shared/)).toBeTruthy();
    // It disappears when the status line does.
    act(() => applyVoiceTimerEvent('VOICE_CHANNEL_STATUS_UPDATE', { id: '77', status: null }));
    expect(screen.queryByText(/shared by someone/)).toBeNull();
  });
});

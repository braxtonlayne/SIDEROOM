// The Activity panel above the call, the launcher and the call-bar button (DEV-ACT-004).
import { ThemeProvider } from '@sideroom/ui';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { emitDispatch } from '../../store/events';
import type { ActivityApi } from '../api';
import { ActivityLauncher } from '../ActivityLauncher';
import { ActivityPanel } from '../ActivityPanel';
import type { ActivityInstance } from '../types';

vi.mock('../../lounge/data', () => ({
  useCurrentUser: () => ({ id: '10', username: 'ann', displayName: 'Ann' }),
  useUsers: () => ({}),
}));

const inst = (over: Partial<ActivityInstance> = {}): ActivityInstance => ({
  id: '9',
  application_id: '7',
  application_name: 'Chess',
  guild_id: '2',
  channel_id: '3',
  started_by: '10',
  created_at_ms: 0,
  participants: [{ user_id: '10', joined_at_ms: 1 }],
  ...over,
});

function fakeApi(over: Partial<ActivityApi> = {}): ActivityApi {
  return {
    shelf: vi.fn(async () => ({
      enabled: true,
      items: [{ application_id: '7', name: 'Chess', description: 'Play chess' }],
    })),
    inChannel: vi.fn(async () => ({ enabled: true, instance: inst() })),
    launch: vi.fn(async () => inst()),
    join: vi.fn(async () => inst()),
    leave: vi.fn(async () => undefined),
    end: vi.fn(async () => undefined),
    ...over,
  };
}

const wrap = (ui: React.ReactNode) => render(<ThemeProvider section="lounge">{ui}</ThemeProvider>);

describe('ActivityPanel', () => {
  it('shows the sandboxed frame on the Activity origin once you joined', async () => {
    wrap(<ActivityPanel channelId="3" channelName="Lobby" here api={fakeApi()} />);
    const frame = await screen.findByTitle('Chess (Activity)');
    expect(frame.tagName).toBe('IFRAME');
    expect(frame.getAttribute('src')).toBe(
      `${window.location.protocol}//7.activities.${window.location.host}/`,
    );
    const sandbox = frame.getAttribute('sandbox') ?? '';
    expect(sandbox.split(' ').sort()).toEqual([
      'allow-forms',
      'allow-popups',
      'allow-same-origin',
      'allow-scripts',
    ]);
    expect(sandbox).not.toContain('allow-top-navigation');
    expect(frame.getAttribute('allow')).toBe('fullscreen');
    expect(frame.getAttribute('referrerpolicy')).toBe('no-referrer');
  });

  it('offers Join (and no frame) while you are not in the Activity', async () => {
    const api = fakeApi({
      inChannel: vi.fn(async () => ({
        enabled: true,
        instance: inst({ started_by: '11', participants: [{ user_id: '11', joined_at_ms: 1 }] }),
      })),
    });
    wrap(<ActivityPanel channelId="3" channelName="Lobby" here api={api} />);
    const join = await screen.findByRole('button', { name: 'Join' });
    expect(screen.queryByTitle('Chess (Activity)')).toBeNull();
    await userEvent.click(join);
    expect(api.join).toHaveBeenCalledWith('9');
  });

  it('lets the starter end it, and follows the gateway when it ends', async () => {
    const api = fakeApi();
    wrap(<ActivityPanel channelId="3" channelName="Lobby" here api={api} />);
    await userEvent.click(await screen.findByRole('button', { name: 'End for everyone' }));
    expect(api.end).toHaveBeenCalledWith('9');
    act(() => emitDispatch('SR_ACTIVITY_INSTANCE_UPDATE', { channel_id: '3', instance: null }));
    await waitFor(() => expect(screen.queryByTitle('Chess (Activity)')).toBeNull());
  });

  it('renders nothing when Activities are off, not in the call, or nothing runs', async () => {
    const off = fakeApi({ inChannel: vi.fn(async () => ({ enabled: false, instance: null })) });
    const { container, rerender } = wrap(
      <ActivityPanel channelId="3" channelName="Lobby" here api={off} />,
    );
    await waitFor(() => expect(off.inChannel).toHaveBeenCalled());
    expect(container.querySelector('.act-panel')).toBeNull();
    rerender(
      <ThemeProvider section="lounge">
        <ActivityPanel channelId="3" channelName="Lobby" here={false} api={fakeApi()} />
      </ThemeProvider>,
    );
    await new Promise((r) => setTimeout(r, 20));
    expect(container.querySelector('.act-panel')).toBeNull();
  });
});

describe('ActivityLauncher', () => {
  it('lists the shelf and starts an Activity', async () => {
    const api = fakeApi();
    const onStarted = vi.fn();
    const onClose = vi.fn();
    wrap(<ActivityLauncher open onClose={onClose} channelId="3" onStarted={onStarted} api={api} />);
    await userEvent.click(await screen.findByRole('button', { name: 'Start Chess' }));
    expect(api.launch).toHaveBeenCalledWith('3', '7');
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(onStarted).toHaveBeenCalledWith(expect.objectContaining({ id: '9' }));
  });

  it('says when nothing is available', async () => {
    const api = fakeApi({ shelf: vi.fn(async () => ({ enabled: true, items: [] })) });
    wrap(<ActivityLauncher open onClose={() => undefined} channelId="3" api={api} />);
    expect(await screen.findByText('No Activities are available yet.')).toBeInTheDocument();
  });
});

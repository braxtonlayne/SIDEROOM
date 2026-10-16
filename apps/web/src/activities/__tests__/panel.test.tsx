// The Activity panel, the shell's dock window and the launcher (DEV-ACT-004, J-15).
import { ThemeProvider } from '@sideroom/ui';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, vi } from 'vitest';
import { emitDispatch } from '../../store/events';
import type { ActivityApi } from '../api';
import { ActivityDock, ActivityWindow } from '../ActivityDock';
import { ActivityLauncher } from '../ActivityLauncher';
import { ActivityPanel } from '../ActivityPanel';
import { useActivitySession } from '../session';
import type { ActivityInstance } from '../types';

vi.mock('../../lounge/data', () => ({
  useCurrentUser: () => ({ id: '10', username: 'ann', displayName: 'Ann' }),
  useUsers: () => ({}),
}));

const goto = vi.fn();
vi.mock('../../lounge/nav', () => ({ useLoungeGoto: () => goto }));

const inst = (over: Partial<ActivityInstance> = {}): ActivityInstance => ({
  id: '9',
  application_id: '7',
  application_name: 'Chess',
  guild_id: '2',
  channel_id: '3',
  channel_type: 'voice',
  development: false,
  started_by: '10',
  created_at_ms: 0,
  participants: [{ user_id: '10', joined_at_ms: 1 }],
  ...over,
});

function fakeApi(over: Partial<ActivityApi> = {}): ActivityApi {
  return {
    shelf: vi.fn(async () => ({
      enabled: true,
      items: [
        { application_id: '7', name: 'Chess', description: 'Play chess', development: false },
        { application_id: '8', name: 'Wip', description: '', development: true },
      ],
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
const tree = (ui: React.ReactNode) => <ThemeProvider section="lounge">{ui}</ThemeProvider>;

/** The panel of a channel view (when shown) beside the shell's dock, in a tree whose shape never changes. */
function Shell({ panel, api }: { panel: boolean; api: ActivityApi }) {
  return (
    <>
      {panel ? <ActivityPanel channelId="3" here api={api} /> : null}
      <ActivityDock api={api} />
    </>
  );
}

afterEach(() => vi.unstubAllGlobals());

beforeEach(() => {
  goto.mockClear();
  useActivitySession.setState({ instance: null, slot: null });
});

describe('ActivityPanel and the dock', () => {
  it('shows the sandboxed frame on the Activity origin once you joined', async () => {
    const api = fakeApi();
    wrap(
      <>
        <ActivityPanel channelId="3" here api={api} />
        <ActivityDock api={api} />
      </>,
    );
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
    // The channel view reserves a slot and the window covers it.
    expect(screen.getByTestId('activity-slot')).toBeInTheDocument();
    expect(frame.closest('.act-window')).toHaveAttribute('data-mode', 'full');
  });

  it('keeps the same frame when you navigate away, docked with Return and Leave', async () => {
    const api = fakeApi();
    const { rerender } = wrap(<Shell panel api={api} />);
    const frame = await screen.findByTitle('Chess (Activity)');
    expect(frame.closest('.act-window')).toHaveAttribute('data-mode', 'full');

    // Another channel view takes over: the panel (and its slot) is gone, the dock stays.
    rerender(tree(<Shell panel={false} api={api} />));
    const again = screen.getByTitle('Chess (Activity)');
    expect(again).toBe(frame);
    expect(document.body.contains(frame)).toBe(true);
    expect(frame.closest('.act-window')).toHaveAttribute('data-mode', 'docked');

    await userEvent.click(screen.getByRole('button', { name: 'Return' }));
    expect(goto).toHaveBeenCalledWith('2', '3');

    // Back in the channel: full size again, still the same element.
    rerender(tree(<Shell panel api={api} />));
    await waitFor(() =>
      expect(screen.getByTitle('Chess (Activity)').closest('.act-window')).toHaveAttribute(
        'data-mode',
        'full',
      ),
    );
    expect(screen.getByTitle('Chess (Activity)')).toBe(frame);

    rerender(tree(<Shell panel={false} api={api} />));
    await userEvent.click(screen.getByRole('button', { name: 'Leave' }));
    expect(api.leave).toHaveBeenCalledWith('9');
    await waitFor(() => expect(screen.queryByTitle('Chess (Activity)')).toBeNull());
  });

  it('follows the gateway from anywhere: another channel ending it closes the window', async () => {
    const api = fakeApi();
    const { rerender } = wrap(<Shell panel api={api} />);
    await screen.findByTitle('Chess (Activity)');
    rerender(tree(<Shell panel={false} api={api} />));
    act(() => emitDispatch('SR_ACTIVITY_INSTANCE_UPDATE', { channel_id: '3', instance: null }));
    await waitFor(() => expect(screen.queryByTitle('Chess (Activity)')).toBeNull());
    // An instance you are in, announced while you are elsewhere, opens the window.
    act(() => emitDispatch('SR_ACTIVITY_INSTANCE_UPDATE', { channel_id: '3', instance: inst() }));
    expect(await screen.findByTitle('Chess (Activity)')).toBeInTheDocument();
    // One you are not in does not.
    act(() =>
      emitDispatch('SR_ACTIVITY_INSTANCE_UPDATE', {
        channel_id: '5',
        instance: inst({
          id: '12',
          channel_id: '5',
          participants: [{ user_id: '11', joined_at_ms: 1 }],
        }),
      }),
    );
    expect(screen.getAllByTitle('Chess (Activity)')).toHaveLength(1);
  });

  it('offers Join (and no frame) while you are not in the Activity', async () => {
    const api = fakeApi({
      inChannel: vi.fn(async () => ({
        enabled: true,
        instance: inst({ started_by: '11', participants: [{ user_id: '11', joined_at_ms: 1 }] }),
      })),
    });
    wrap(
      <>
        <ActivityPanel channelId="3" here api={api} />
        <ActivityDock api={api} />
      </>,
    );
    const join = await screen.findByRole('button', { name: 'Join' });
    expect(screen.queryByTitle('Chess (Activity)')).toBeNull();
    await userEvent.click(join);
    expect(api.join).toHaveBeenCalledWith('9');
  });

  it('lets the starter end it, and follows the gateway when it ends', async () => {
    const api = fakeApi();
    wrap(
      <>
        <ActivityPanel channelId="3" here api={api} />
        <ActivityDock api={api} />
      </>,
    );
    await userEvent.click(await screen.findByRole('button', { name: 'End for everyone' }));
    expect(api.end).toHaveBeenCalledWith('9');
    act(() => emitDispatch('SR_ACTIVITY_INSTANCE_UPDATE', { channel_id: '3', instance: null }));
    await waitFor(() => expect(screen.queryByTitle('Chess (Activity)')).toBeNull());
  });

  it('marks a development build', async () => {
    const api = fakeApi({
      inChannel: vi.fn(async () => ({ enabled: true, instance: inst({ development: true }) })),
    });
    wrap(<ActivityPanel channelId="3" here api={api} />);
    expect(await screen.findByText('Development')).toBeInTheDocument();
  });

  it('shows a text-channel Activity without needing a call', async () => {
    const api = fakeApi({
      inChannel: vi.fn(async () => ({
        enabled: true,
        instance: inst({ channel_type: 'text' }),
      })),
    });
    wrap(
      <>
        <ActivityPanel channelId="3" here api={api} />
        <ActivityDock api={api} />
      </>,
    );
    expect(await screen.findByTitle('Chess (Activity)')).toBeInTheDocument();
  });

  it('renders nothing when Activities are off, not in the call, or nothing runs', async () => {
    const off = fakeApi({ inChannel: vi.fn(async () => ({ enabled: false, instance: null })) });
    const { container, rerender } = wrap(<ActivityPanel channelId="3" here api={off} />);
    await waitFor(() => expect(off.inChannel).toHaveBeenCalled());
    expect(container.querySelector('.act-panel')).toBeNull();
    rerender(tree(<ActivityPanel channelId="3" here={false} api={fakeApi()} />));
    await new Promise((r) => setTimeout(r, 20));
    expect(container.querySelector('.act-panel')).toBeNull();
  });
});

describe('ActivityWindow', () => {
  it('moves between full and docked on one element without remounting the frame', () => {
    const props = { instance: inst(), channelName: 'Lobby', onReturn: vi.fn(), onLeave: vi.fn() };
    const { rerender } = wrap(
      <ActivityWindow {...props} rect={{ left: 10, top: 20, width: 600, height: 400 }} />,
    );
    const frame = screen.getByTitle('Chess (Activity)');
    const box = frame.closest('.act-window') as HTMLElement;
    expect(box.dataset.mode).toBe('full');
    expect(box.style.width).toBe('600px');
    expect(box.style.height).toBe('400px');
    rerender(tree(<ActivityWindow {...props} rect={null} />));
    expect(screen.getByTitle('Chess (Activity)')).toBe(frame);
    expect(frame.closest('.act-window')).toBe(box);
    expect(box.dataset.mode).toBe('docked');
    expect(box.style.width).toBe('');
  });

  it('drags by its bar, inside the viewport', () => {
    // jsdom has no PointerEvent; a MouseEvent carries the coordinates React reads.
    vi.stubGlobal('PointerEvent', class extends MouseEvent {});
    wrap(
      <ActivityWindow
        instance={inst()}
        channelName="Lobby"
        rect={null}
        onReturn={() => undefined}
        onLeave={() => undefined}
      />,
    );
    const box = screen.getByRole('region', { name: 'Chess Activity window' });
    const bar = box.querySelector('.act-window-bar') as HTMLElement;
    const before = box.style.transform;
    act(() => {
      bar.dispatchEvent(
        new PointerEvent('pointerdown', { bubbles: true, clientX: 700, clientY: 500 }),
      );
      bar.dispatchEvent(
        new PointerEvent('pointermove', { bubbles: true, clientX: 100, clientY: 100 }),
      );
      bar.dispatchEvent(new PointerEvent('pointerup', { bubbles: true }));
    });
    expect(box.style.transform).not.toBe(before);
    // Never off-screen to the top or left.
    const m = /translate\((-?[\d.]+)px, (-?[\d.]+)px\)/.exec(box.style.transform);
    expect(Number(m?.[1])).toBeGreaterThanOrEqual(0);
    expect(Number(m?.[2])).toBeGreaterThanOrEqual(0);
  });
});

describe('ActivityLauncher', () => {
  it('lists the shelf, marks development builds and starts an Activity', async () => {
    const api = fakeApi();
    const onStarted = vi.fn();
    const onClose = vi.fn();
    wrap(<ActivityLauncher open onClose={onClose} channelId="3" onStarted={onStarted} api={api} />);
    expect(await screen.findByText('Development')).toBeInTheDocument();
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

import { act, render, screen } from '@testing-library/react';
import { ToastProvider } from '@sideroom/ui';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { resetStore, useStore } from '../../store';
import { emitDispatch } from '../../store/events';
import { DmToasts } from '../shell/DmToasts';
import { useNotifSettings } from '../shell/notifSettings';

let activeChannel: string | undefined;
vi.mock('../nav', async (orig) => ({
  ...(await orig<typeof import('../nav')>()),
  useLoungeRoute: () => ({ guildId: '@me', channelId: activeChannel }),
}));

const dm = (over: Record<string, unknown> = {}) => ({
  id: 'm1',
  channel_id: 'dm1',
  author: { id: 'u2', username: 'bob', global_name: 'Bob' },
  content: 'are you there?',
  timestamp: '2026-01-01T00:00:00Z',
  mentions: [],
  ...over,
});

async function send(m: unknown) {
  await act(async () => {
    emitDispatch('MESSAGE_CREATE', m);
    await Promise.resolve();
    await Promise.resolve();
  });
}

beforeEach(() => {
  resetStore();
  useStore.setState({ currentUserId: 'u1' });
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
  useNotifSettings.setState({ load: async () => undefined, byGuild: {} });
  activeChannel = 'other';
  vi.spyOn(document, 'hasFocus').mockReturnValue(true);
});
afterEach(() => vi.restoreAllMocks());

const mount = () =>
  render(
    <ToastProvider>
      <DmToasts />
    </ToastProvider>,
  );

describe('DM toasts (LNG-NTF-009)', () => {
  it('shows a toast for a DM while another channel is open', async () => {
    mount();
    await send(dm());
    expect(await screen.findByText('Bob sent you a message')).toBeInTheDocument();
    expect(screen.getByText('are you there?')).toBeInTheDocument();
  });

  it('stays quiet for the open conversation, server messages, own messages and the off switch', async () => {
    mount();
    activeChannel = undefined;
    await send(dm({ channel_id: 'other' }));
    await send(dm({ guild_id: 'g1' }));
    await send(dm({ author: { id: 'u1', username: 'me' } }));
    expect(screen.queryByText(/sent you a message/)).toBeNull();
  });

  it('respects the preference, Do Not Disturb and a window in the background', async () => {
    mount();
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, dmToasts: false } });
    await send(dm());
    usePrefsStore.setState({ prefs: DEFAULT_PREFS });
    useStore.setState({ presences: { u1: { status: 'dnd' } } } as never);
    await send(dm());
    useStore.setState({ presences: {} } as never);
    vi.spyOn(document, 'hasFocus').mockReturnValue(false);
    await send(dm());
    expect(screen.queryByText(/sent you a message/)).toBeNull();
  });
});

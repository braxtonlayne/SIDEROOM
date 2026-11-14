// SEC-AUD-284 (wave 73, D-764): the "Thread started from" line is D-729's plain line: no markup, a spoiler hidden,
// a mention as a name, one line, nothing of a blocked or ignored author's text, and empty before the parser is ready.
import { render, screen, waitFor } from '@testing-library/react';
import { DEFAULT_PREFS, usePrefsStore } from '../../settings/user/prefs';
import { resetStore, useStore } from '../../store';
import { http, HttpResponse, server } from '../../test/server';
import { setMarkdownEngine, testFallbackEngine } from '../markdown';
import { SystemMessage } from '../msg/SystemMessage';
import { useIgnores } from '../shell/ignores';
import { useSocial } from '../shell/social';

const RAW = 'see ||the ending|| and `code` <@123>';

beforeEach(() => {
  resetStore();
  setMarkdownEngine(testFallbackEngine);
  useSocial.getState().reset();
  useIgnores.getState().reset();
  usePrefsStore.setState({ prefs: DEFAULT_PREFS });
  useStore.setState({
    currentUserId: '1',
    guilds: { g1: { id: 'g1', name: 'G', owner_id: '1' } },
    channels: { c1: { id: 'c1', type: 0, guild_id: 'g1', name: 'general' } },
    channelIdsByGuild: { g1: ['c1'] },
    users: {
      '1': { id: '1', username: 'me' },
      '2': { id: '2', username: 'bob', global_name: 'Bob' },
      '123': { id: '123', username: 'dana', global_name: 'Dana' },
    },
  } as never);
  server.use(http.get('*/api/v1/sr/csrf', () => HttpResponse.json({ token: 't' })));
});

function starter(content: string, authorId = '2') {
  server.use(
    http.get('*/api/v1/channels/c1/messages/55', () =>
      HttpResponse.json({
        id: '55',
        channel_id: 'c1',
        content,
        author: { id: authorId, username: 'bob', global_name: 'Bob' },
      }),
    ),
  );
  return render(
    <SystemMessage
      message={
        { id: '100', type: 21, content: '', author: { id: '2', displayName: 'Bob' } } as never
      }
      referenceId="55"
      referenceChannelId="c1"
    />,
  );
}
const quote = async (view: ReturnType<typeof render>) => {
  await waitFor(() => expect(view.container.querySelector('q')).not.toBeNull());
  return view.container.querySelector('q') as HTMLElement;
};

describe('the thread starter line (D-764)', () => {
  it('hides a spoiler, drops the backticks and names a mention', async () => {
    const q = await quote(starter(RAW));
    expect(q.textContent).toBe('see [spoiler] and code @Dana');
    expect(q.textContent).not.toContain('the ending');
    expect(q.textContent).not.toContain('`');
    expect(q.textContent).not.toContain('<@');
  });

  it('shows a plain sentence as it is', async () => {
    expect((await quote(starter('hello there'))).textContent).toBe('hello there');
  });

  it('keeps the attachment wording for a starter without text', async () => {
    expect((await quote(starter(''))).textContent).toBe('Attachment');
  });

  it('shows nothing of a blocked author', async () => {
    useSocial.setState({
      relationships: [{ id: '2', type: 2, since: '', user: { id: '2', username: 'bob' } }],
    } as never);
    const q = await quote(starter(RAW));
    expect(q.textContent).toBe('1 blocked message');
    expect(q.textContent).not.toContain('ending');
    expect(q.textContent).not.toContain('see');
  });

  it('is empty before the parser is ready, never the typed text', async () => {
    setMarkdownEngine(null);
    const q = await quote(starter(RAW));
    expect(q.textContent).toBe('');
    expect(screen.queryByText(/the ending/)).toBeNull();
  });
});

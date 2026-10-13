import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { useStore } from '../../store';
import { usePrefsStore, DEFAULT_PREFS } from '../../settings/user/prefs';
import { http, HttpResponse, server } from '../../test/server';

import { at, baseHandlers, bob, calls, me, msg, record, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

describe('message list gaps', () => {
  beforeEach(() => baseHandlers());

  it('loads the next page when scrolled to the bottom of a jumped-to window', async () => {
    seedLounge({
      messages: [msg('100', { content: 'old one' }), msg('101', { content: 'old two' })],
      atLatest: false,
    });
    server.use(
      http.get('*/api/v1/channels/c1/messages', async (ctx) => {
        await record('GET')(ctx);
        // Discord returns newest first.
        return HttpResponse.json([
          msg('103', { content: 'newest' }),
          msg('102', { content: 'mid' }),
        ]);
      }),
    );
    at('/channels/g1/c1');
    const log = await screen.findByRole('log', { name: 'Messages in general' });
    expect(await within(log).findByText('old two')).toBeInTheDocument();
    expect(screen.getByText(/viewing older messages/i)).toBeInTheDocument();
    fireEvent.scroll(log);
    expect(await within(log).findByText('newest')).toBeInTheDocument();
    expect(within(log).getByText('mid')).toBeInTheDocument();
    expect(calls.some((c) => c.key === 'GET /channels/c1/messages?limit=50&after=101')).toBe(true);
    // A short page reaches the newest message: live mode resumes and the jump bar goes away.
    await waitFor(() => expect(useStore.getState().windows['c1']?.atLatest).toBe(true));
    expect(screen.queryByText(/viewing older messages/i)).not.toBeInTheDocument();
  });

  it('collapses blocked and ignored authors until the viewer shows them', async () => {
    seedLounge({
      messages: [
        msg('100', { content: 'visible hello', author: me }),
        msg('101', { content: 'secret one' }),
        msg('102', { content: 'secret two' }),
        msg('103', { content: 'after', author: me }),
      ],
    });
    server.use(http.get('*/api/v1/users/@me/ignores', () => HttpResponse.json([bob])));
    at('/channels/g1/c1');
    const log = await screen.findByRole('log', { name: 'Messages in general' });
    expect(await within(log).findByText('2 ignored messages')).toBeInTheDocument();
    expect(within(log).queryByText('secret one')).not.toBeInTheDocument();
    expect(within(log).getByText('visible hello')).toBeInTheDocument();
    fireEvent.click(within(log).getByRole('button', { name: 'Show messages' }));
    expect(await within(log).findByText('secret one')).toBeInTheDocument();
    expect(within(log).getByText('secret two')).toBeInTheDocument();
  });

  it('shows the replied-to author avatar, role icon and gradient', async () => {
    seedLounge({
      messages: [
        msg('100', { content: 'the original' }),
        msg('101', {
          author: me,
          content: 'a reply',
          type: 19,
          message_reference: { message_id: '100' },
        }),
      ],
      roles: [
        {
          id: 'r1',
          guild_id: 'g1',
          name: 'Mods',
          color: 0xff0000,
          position: 2,
          permissions: '0',
          unicode_emoji: '\u{1F6E1}',
          colors: { primary_color: 0xff0000, secondary_color: 0x0000ff },
        },
      ],
      members: {
        'g1:1': { guild_id: 'g1', user_id: '1', roles: [] },
        'g1:2': { guild_id: 'g1', user_id: '2', roles: ['r1'] },
      },
    });
    const { container } = at('/channels/g1/c1');
    const log = await screen.findByRole('log', { name: 'Messages in general' });
    await within(log).findByText('a reply');
    expect(log.querySelector('.lng-reply .lng-reply-avatar')).not.toBeNull();
    const author = Array.from(log.querySelectorAll('.lng-msg-author')).find(
      (e) => e.textContent === 'Bob',
    ) as HTMLElement;
    expect(author).toHaveAttribute('data-gradient');
    expect(author.style.backgroundImage).toContain('linear-gradient');
    expect(within(log).getByRole('img', { name: 'Mods icon' })).toHaveTextContent('\u{1F6E1}');
    expect(await axeViolations(container)).toEqual([]);
  });

  it('compact display drops the avatar and puts the time on every line', async () => {
    seedLounge({
      messages: [msg('100', { content: 'one' }), msg('101', { content: 'two' })],
    });
    usePrefsStore.setState({ prefs: { ...DEFAULT_PREFS, messageDisplay: 'compact' } });
    at('/channels/g1/c1');
    const log = await screen.findByRole('log', { name: 'Messages in general' });
    await within(log).findByText('two');
    const rows = log.querySelectorAll('article.lng-msg');
    expect(rows).toHaveLength(2);
    for (const r of rows) {
      expect(r).toHaveAttribute('data-compact');
      expect(r.querySelector('.lng-msg-author')).not.toBeNull();
      expect(r.querySelector('.lng-msg-hover-time')).not.toBeNull();
    }
  });
});

import { screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../test/server';
import { at, baseHandlers, calls, record, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

const entry = (id: string, name: string, over: Record<string, unknown> = {}) => ({
  id,
  name,
  description: `${name} blurb`,
  icon: null,
  banner: null,
  category: 'gaming',
  featured: false,
  approximate_member_count: 12,
  approximate_presence_count: 3,
  preferred_locale: 'en-US',
  vanity_url_code: null,
  joined: false,
  ...over,
});

describe('LNG-RAIL-011 Server Directory', () => {
  // The shell is a lazy route; load it once up front so a cold run does not eat the first test's budget.
  beforeAll(async () => {
    await import('../LoungeShell');
  }, 90_000);

  beforeEach(() => {
    baseHandlers();
    seedLounge({ messages: [] });
    server.use(
      http.get('*/api/v1/directory/categories', () => HttpResponse.json(['gaming', 'music'])),
      http.get('*/api/v1/directory', async (ctx) => {
        await record('GET')(ctx);
        const q = new URL(ctx.request.url).searchParams.get('query');
        const all = [entry('d1', 'Chess Club', { featured: true }), entry('d2', 'Jazz Hands')];
        return HttpResponse.json(q ? all.filter((e) => e.name.toLowerCase().includes(q)) : all);
      }),
    );
  });

  it('opens from the compass button, lists servers and searches', async () => {
    const { container } = at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Discover' }));
    const dlg = await screen.findByRole('dialog', { name: 'Server Directory' });
    const list = await within(dlg).findByRole('list', { name: 'Listed servers' });
    expect(within(list).getByText('Chess Club')).toBeInTheDocument();
    expect(within(list).getByText('Featured')).toBeInTheDocument();
    expect(within(list).getAllByRole('button', { name: 'Join' })).toHaveLength(2);
    expect(await axeViolations(container)).toEqual([]);

    await userEvent.type(within(dlg).getByRole('searchbox'), 'jazz');
    await waitFor(() => expect(within(list).queryByText('Chess Club')).toBeNull(), {
      timeout: 4000,
    });
    expect(calls.some((c) => c.key.includes('query=jazz'))).toBe(true);
  });

  it('filters by category and shows the empty state', async () => {
    server.use(http.get('*/api/v1/directory', () => HttpResponse.json([])));
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Discover' }));
    const dlg = await screen.findByRole('dialog', { name: 'Server Directory' });
    expect(await within(dlg).findByText('No servers found')).toBeInTheDocument();
    const music = await within(dlg).findByRole('button', { name: 'Music' });
    await userEvent.click(music);
    expect(music).toHaveAttribute('aria-pressed', 'true');
  });

  it('joins a listed server without an invite', async () => {
    server.use(
      http.post('*/api/v1/directory/d2/join', async (ctx) => {
        await record('POST')(ctx);
        return HttpResponse.json({ user: { id: '1', username: 'me' }, roles: [] });
      }),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Discover' }));
    const dlg = await screen.findByRole('dialog', { name: 'Server Directory' });
    const card = await within(dlg).findByRole('listitem', { name: 'Jazz Hands' });
    await userEvent.click(within(card).getByRole('button', { name: 'Join' }));
    await waitFor(() => expect(calls.some((c) => c.key === 'POST /directory/d2/join')).toBe(true));
  });

  it('reports a failed join on the card', async () => {
    server.use(
      http.post('*/api/v1/directory/d1/join', () =>
        HttpResponse.json(
          { code: 40007, message: 'You are banned from this server.' },
          { status: 403 },
        ),
      ),
    );
    at('/channels/g1/c1');
    await userEvent.click(await screen.findByRole('button', { name: 'Discover' }));
    const dlg = await screen.findByRole('dialog', { name: 'Server Directory' });
    const card = await within(dlg).findByRole('listitem', { name: 'Chess Club' });
    await userEvent.click(within(card).getByRole('button', { name: 'Join' }));
    expect(await within(card).findByRole('alert')).toBeInTheDocument();
  });
});

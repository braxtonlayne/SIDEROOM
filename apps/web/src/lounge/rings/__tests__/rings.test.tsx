// Status rings UI (PRF-RNG-001): the ring on an avatar, the viewer with the author-only viewer list, the tray, the
// composer, and reduced motion.
import { ThemeProvider } from '@sideroom/ui';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../../test/server';
import type { Ring, RingFeedEntry } from '../api';
import { RingAvatar } from '../RingAvatar';
import { RingComposer } from '../RingComposer';
import { refreshRings, resetRings } from '../store';
import { RingTray } from '../RingTray';

vi.mock('../../data', () => ({
  useCurrentUser: () => ({ id: '1', username: 'me', displayName: 'Me', avatarUrl: null }),
  useUsers: () => ({
    '2': { id: '2', username: 'bo', displayName: 'Bo', avatarUrl: null },
    '3': { id: '3', username: 'cy', displayName: 'Cy', avatarUrl: null },
  }),
  useGuilds: () => [
    { id: 'g1', name: 'Club One' },
    { id: 'g2', name: 'Club Two' },
  ],
}));

const ring = (id: string, over: Partial<Ring> = {}): Ring => ({
  id,
  author_id: '2',
  kind: 'text',
  body: `words ${id}`,
  track: null,
  has_media: false,
  content_type: null,
  duration_ms: null,
  audience: 'friends',
  guild_ids: [],
  created_at: '2026-10-30T10:00:00.000Z',
  expires_at: '2026-10-31T10:00:00.000Z',
  seen: false,
  mine: false,
  ...over,
});

const bo: RingFeedEntry = {
  author_id: '2',
  all_seen: false,
  rings: [
    ring('10'),
    ring('11', { kind: 'track', body: '', track: { title: 'Song', artist: 'Band' } }),
  ],
};
const mine: RingFeedEntry = {
  author_id: '1',
  all_seen: true,
  rings: [
    ring('20', { author_id: '1', mine: true, seen: true, guild_ids: ['g1'], audience: 'guilds' }),
  ],
};

const calls: string[] = [];
function feed(entries: RingFeedEntry[] | 'fail') {
  server.use(
    http.get('*/api/v1/rings', () =>
      entries === 'fail'
        ? HttpResponse.json({ message: 'Unknown' }, { status: 403 })
        : HttpResponse.json(entries),
    ),
    http.post('*/api/v1/rings/:id/view', ({ params }) => {
      calls.push(`view ${String(params.id)}`);
      return new HttpResponse(null, { status: 204 });
    }),
    http.get('*/api/v1/users/@me/rings/:id/viewers', ({ params }) => {
      calls.push(`viewers ${String(params.id)}`);
      return HttpResponse.json([{ user_id: '3', viewed_at: '2026-10-30T11:00:00.000Z' }]);
    }),
  );
}

const wrap = (ui: React.ReactNode) => render(<ThemeProvider section="lounge">{ui}</ThemeProvider>);

beforeEach(() => {
  calls.length = 0;
  resetRings();
});

describe('the ring on an avatar', () => {
  it('is decoration only inside a link: no nested control, a spoken label, no axe violations', async () => {
    feed([bo]);
    const { container } = wrap(
      <a href="/dm">
        <RingAvatar userId="2" name="Bo" interactive={false}>
          <span>B</span>
        </RingAvatar>
      </a>,
    );
    await refreshRings();
    expect(await screen.findByText('Bo has a new ring')).toBeInTheDocument();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
    expect(container.querySelector('a button')).toBeNull();
    expect(await axeViolations(container)).toEqual([]);
  });

  it('draws nothing for a person without a live ring', async () => {
    feed([bo]);
    wrap(
      <RingAvatar userId="3" name="Cy">
        <span>C</span>
      </RingAvatar>,
    );
    await refreshRings();
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('opens the viewer, notes the view, plays the rings in order and never loads a viewer list for them', async () => {
    feed([bo]);
    wrap(
      <RingAvatar userId="2" name="Bo">
        <span>B</span>
      </RingAvatar>,
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Bo has a new ring' }));
    expect(await screen.findByText('words 10')).toBeInTheDocument();
    await waitFor(() => expect(calls).toContain('view 10'));
    await userEvent.click(screen.getByRole('button', { name: 'Next ring' }));
    expect(await screen.findByText('Song')).toBeInTheDocument();
    await waitFor(() => expect(calls).toContain('view 11'));
    expect(screen.queryByText(/Opened by/)).not.toBeInTheDocument();
    expect(calls.some((c) => c.startsWith('viewers'))).toBe(false);
    expect(screen.queryByRole('button', { name: 'Delete this ring' })).not.toBeInTheDocument();
  });
});

describe('the author’s own viewer', () => {
  it('lists who opened the ring, says only the author sees it, and deletes the ring', async () => {
    feed([mine]);
    let deleted = false;
    server.use(
      http.delete('*/api/v1/users/@me/rings/:id', () => {
        deleted = true;
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const { baseElement } = wrap(
      <RingAvatar userId="1" name="Me">
        <span>M</span>
      </RingAvatar>,
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Me has a ring' }));
    expect(await screen.findByText('Opened by 1 person')).toBeInTheDocument();
    expect(screen.getByText('Cy')).toBeInTheDocument();
    expect(screen.getByText('Only you can see this list.')).toBeInTheDocument();
    expect(screen.getByText(/Shown to 1 server/)).toBeInTheDocument();
    expect(calls).toContain('viewers 20');
    expect(calls.some((c) => c.startsWith('view '))).toBe(false);
    expect(await axeViolations(baseElement)).toEqual([]);
    await userEvent.click(screen.getByRole('button', { name: 'Delete this ring' }));
    await waitFor(() => expect(deleted).toBe(true));
  });
});

describe('the tray', () => {
  it('renders nothing when the feed cannot be loaded (rings off)', async () => {
    feed('fail');
    const { container } = wrap(<RingTray />);
    await refreshRings();
    await waitFor(() => expect(container.querySelector('.rng-tray')).toBeNull());
  });

  it('shows the add button, your ring and everyone else’s', async () => {
    feed([mine, bo]);
    const { container } = wrap(<RingTray />);
    expect(await screen.findByRole('button', { name: 'Add a ring' })).toBeInTheDocument();
    expect(await screen.findByRole('button', { name: 'Bo has a new ring' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Me has a ring' })).toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe('the composer', () => {
  it('posts a text ring to friends', async () => {
    let body: unknown;
    server.use(
      http.post('*/api/v1/users/@me/rings', async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(ring('30', { mine: true }), { status: 201 });
      }),
      http.get('*/api/v1/rings', () => HttpResponse.json([])),
    );
    const onClose = vi.fn();
    wrap(<RingComposer onClose={onClose} />);
    await userEvent.type(screen.getByLabelText('Your words'), 'good morning');
    await userEvent.click(screen.getByRole('button', { name: 'Post ring' }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(body).toEqual({
      kind: 'text',
      text: 'good morning',
      audience: 'friends',
      guild_ids: [],
    });
  });

  it('asks for at least one server before posting to servers, then sends the chosen ones', async () => {
    let body: { guild_ids?: string[]; audience?: string } = {};
    server.use(
      http.post('*/api/v1/users/@me/rings', async ({ request }) => {
        body = (await request.json()) as typeof body;
        return HttpResponse.json(ring('31', { mine: true }), { status: 201 });
      }),
      http.get('*/api/v1/rings', () => HttpResponse.json([])),
    );
    const onClose = vi.fn();
    wrap(<RingComposer onClose={onClose} />);
    await userEvent.type(screen.getByLabelText('Your words'), 'hi servers');
    await userEvent.click(screen.getByRole('radio', { name: 'Members of servers I choose' }));
    await userEvent.click(screen.getByRole('button', { name: 'Post ring' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('Choose at least one server.');
    await userEvent.click(screen.getByRole('checkbox', { name: 'Club Two' }));
    await userEvent.click(screen.getByRole('button', { name: 'Post ring' }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(body.audience).toBe('guilds');
    expect(body.guild_ids).toEqual(['g2']);
  });

  it('says so when the day’s limit is reached', async () => {
    server.use(
      http.post('*/api/v1/users/@me/rings', () =>
        HttpResponse.json({ message: 'limit', code: 0 }, { status: 409 }),
      ),
    );
    wrap(<RingComposer onClose={() => undefined} />);
    await userEvent.type(screen.getByLabelText('Your words'), 'again');
    await userEvent.click(screen.getByRole('button', { name: 'Post ring' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('limit of rings');
  });
});

describe('motion and layout', () => {
  const css = readFileSync(join(__dirname, '..', 'rings.css'), 'utf8');

  it('stops the ring pulse under reduced motion', () => {
    expect(css).toMatch(/@keyframes rng-pulse/);
    expect(css).toMatch(
      /@media \(prefers-reduced-motion: reduce\)\s*\{[^}]*\{[^}]*animation:\s*none/s,
    );
  });

  it('uses logical properties only', () => {
    expect(css).not.toMatch(/\b(margin|padding)-(left|right)\b|\b(left|right):|\b(width|height):/);
  });
});

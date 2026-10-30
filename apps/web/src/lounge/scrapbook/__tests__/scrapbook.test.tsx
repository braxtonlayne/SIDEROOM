// PRF-SCR-001: the scrapbook gallery, the message menu entry, the channel banner and the shelf panel.
import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { http, HttpResponse, server } from '../../../test/server';
import type { ScrapbookEntry, ScrapbookStatus } from '../api';
import { ScrapbookBanner } from '../Banner';
import { EntryTile, Filters, ScrapbookGallery, parseTags } from '../Gallery';
import { RemovalList, ShelfPanel } from '../ShelfPanel';
import { useScrapbookStatus } from '../store';
import { useScrapbookMenuEntry } from '../useSendToScrapbook';

vi.mock('../../data', () => ({
  useChannels: () => [
    { id: 'c1', name: 'general', type: 'text' },
    { id: 'c2', name: 'art', type: 'text' },
  ],
}));

const status = (over: Partial<ScrapbookStatus> = {}): ScrapbookStatus => ({
  offered: true,
  enabled: true,
  emojis: ['\u{1f4cc}'],
  threshold: 3,
  can_send: true,
  can_manage: false,
  failed_removals: 0,
  ignored_channels: [],
  ...over,
});

const entry = (over: Partial<ScrapbookEntry> = {}): ScrapbookEntry => ({
  id: 'e1',
  channel_id: 'c1',
  message_id: 'm1',
  author_id: 'u1',
  author_name: 'Ada',
  content: 'look at this cat',
  sent_at: 1_782_000_000_000,
  tags: ['cats'],
  via: 'reaction',
  skipped: 0,
  can_edit: false,
  objects: [
    {
      id: 'o1',
      filename: 'cat.png',
      content_type: 'image/png',
      size: 10,
      kind: 'image',
      playable: true,
    },
  ],
  ...over,
});

const API = '*/api/v1/guilds/g1/scrapbook';

beforeEach(() => {
  useScrapbookStatus.getState().reset();
});

function serve(s: ScrapbookStatus, pages: Record<string, unknown>, seen: string[] = []) {
  server.use(
    http.get(`${API}/status`, () => HttpResponse.json(s)),
    http.get(API, ({ request }) => {
      const u = new URL(request.url);
      seen.push(u.search);
      const before = u.searchParams.get('before') ?? '';
      return HttpResponse.json(pages[before] ?? { entries: [] });
    }),
    http.post(`${API}/:entry/objects/:object/presign`, () =>
      HttpResponse.json({ url: 'https://bucket.test/cat.png?sig=1', expires_at: 'x' }),
    ),
  );
}

describe('parseTags', () => {
  it('splits on commas and drops hashes and blanks', () => {
    expect(parseTags(' #cats, dogs ,, ')).toEqual(['cats', 'dogs']);
    expect(parseTags('')).toEqual([]);
  });
});

describe('ScrapbookGallery', () => {
  it('says the scrapbook is off when the operator has not offered it', async () => {
    serve(status({ offered: false, enabled: false }), {});
    render(<ScrapbookGallery guildId="g1" />);
    expect(await screen.findByTestId('scr-off')).toBeInTheDocument();
    expect(screen.queryByTestId('scr-grid')).toBeNull();
  });

  it('shows an entry with its text, author, channel, tags and a signed image', async () => {
    serve(status(), { '': { entries: [entry()] } });
    render(<ScrapbookGallery guildId="g1" />);
    const tile = await screen.findByTestId('scr-entry');
    expect(within(tile).getByText('look at this cat')).toBeInTheDocument();
    expect(within(tile).getByText('Ada')).toBeInTheDocument();
    expect(within(tile).getByText(/#general/)).toBeInTheDocument();
    expect(within(tile).getByText('#cats')).toBeInTheDocument();
    const img = await within(tile).findByRole('img', { name: 'cat.png' });
    expect(img).toHaveAttribute('src', 'https://bucket.test/cat.png?sig=1');
  });

  it('shows "not shared with you" in place of a file the viewer\'s storage scopes do not reach', async () => {
    let signed = 0;
    serve(status(), {
      '': {
        entries: [
          entry({
            objects: [
              {
                id: 'o2',
                filename: 'clip.mp4',
                content_type: 'video/mp4',
                size: 9,
                kind: 'video',
                playable: false,
              },
            ],
          }),
        ],
      },
    });
    server.use(
      http.post(`${API}/:entry/objects/:object/presign`, () => {
        signed += 1;
        return HttpResponse.json({ url: 'x', expires_at: 'x' });
      }),
    );
    render(<ScrapbookGallery guildId="g1" />);
    const tile = await screen.findByTestId('scr-entry');
    expect(within(tile).getByTestId('scr-locked')).toHaveTextContent('clip.mp4');
    // The text of the entry is still readable, and no signature was even asked for.
    expect(within(tile).getByText('look at this cat')).toBeInTheDocument();
    expect(signed).toBe(0);
  });

  it('shows "not shared" when the server refuses the signature', async () => {
    serve(status(), { '': { entries: [entry()] } });
    server.use(
      http.post(`${API}/:entry/objects/:object/presign`, () =>
        HttpResponse.json({ message: 'Missing Permissions' }, { status: 403 }),
      ),
    );
    render(<ScrapbookGallery guildId="g1" />);
    expect(await screen.findByTestId('scr-locked')).toBeInTheDocument();
  });

  it('filters by date, channel, tag and member, and pages with "show more"', async () => {
    const seen: string[] = [];
    const facets = {
      tags: [{ tag: 'cats', count: 1 }],
      authors: [{ id: 'u1', name: 'Ada', count: 1 }],
      channels: [{ id: 'c2', count: 1 }],
    };
    serve(
      status(),
      {
        '': { entries: [entry()], next_before: 'e1', facets },
        e1: { entries: [entry({ id: 'e0', content: 'older one', objects: [] })] },
      },
      seen,
    );
    const user = userEvent.setup();
    render(<ScrapbookGallery guildId="g1" />);
    await screen.findByTestId('scr-entry');
    await user.click(await screen.findByRole('button', { name: 'Show more' }));
    expect(await screen.findByText('older one')).toBeInTheDocument();
    expect(seen.at(-1)).toContain('before=e1');

    await user.selectOptions(screen.getByLabelText('Channel'), 'c2');
    await waitFor(() => expect(seen.at(-1)).toContain('channel_id=c2'));
    await user.selectOptions(screen.getByLabelText('Tag'), 'cats');
    await waitFor(() => expect(seen.at(-1)).toContain('tag=cats'));
    await user.selectOptions(screen.getByLabelText('Member'), 'u1');
    await waitFor(() => expect(seen.at(-1)).toContain('author_id=u1'));
    await user.type(screen.getByLabelText('From'), '2026-06-01');
    await waitFor(() => expect(seen.at(-1)).toContain('from=2026-06-01'));
    await user.type(screen.getByLabelText('To'), '2026-06-30');
    await waitFor(() => expect(seen.at(-1)).toContain('to=2026-06-30'));
  });

  it("shows an empty state and the managers' shelf panel only to managers", async () => {
    server.use(
      http.get(`*/api/v1/guilds/g1/storage/mounts`, () => HttpResponse.json([])),
      http.get(`${API}/removals`, () => HttpResponse.json({ removals: [] })),
    );
    serve(status({ can_manage: true }), { '': { entries: [] } });
    render(<ScrapbookGallery guildId="g1" />);
    expect(await screen.findByTestId('scr-empty')).toBeInTheDocument();
    expect(await screen.findByTestId('scr-no-target')).toBeInTheDocument();
  });

  it('has no accessibility violations', async () => {
    serve(status(), { '': { entries: [entry()] } });
    const { container } = render(<ScrapbookGallery guildId="g1" />);
    await screen.findByRole('img', { name: 'cat.png' });
    expect(await axeViolations(container)).toEqual([]);
  });
});

describe('EntryTile actions', () => {
  it('saves tags and removes an entry, telling the member when a copy could not be removed yet', async () => {
    server.use(
      http.patch(`${API}/e1`, async ({ request }) => {
        const body = (await request.json()) as { tags: string[] };
        return HttpResponse.json(entry({ tags: body.tags, can_edit: true }));
      }),
      http.delete(`${API}/e1`, () => HttpResponse.json({ copies_removed: 0, copies_failed: 1 })),
      http.post(`${API}/:entry/objects/:object/presign`, () =>
        HttpResponse.json({ url: 'https://bucket.test/x', expires_at: 'x' }),
      ),
    );
    const onChanged = vi.fn();
    const onRemoved = vi.fn();
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const user = userEvent.setup();
    render(
      <EntryTile
        guildId="g1"
        entry={entry({ can_edit: true })}
        channelName="general"
        onChanged={onChanged}
        onRemoved={onRemoved}
      />,
    );
    await user.click(screen.getByRole('button', { name: 'Edit tags' }));
    const input = screen.getByLabelText('Tags, separated by commas');
    await user.clear(input);
    await user.type(input, '#funny, cats');
    await user.click(screen.getByRole('button', { name: 'Save tags' }));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(onChanged.mock.calls[0]![0].tags).toEqual(['funny', 'cats']);

    await user.click(screen.getByRole('button', { name: 'Remove' }));
    await waitFor(() => expect(onRemoved).toHaveBeenCalledWith('e1', 1));
  });

  it('offers no actions to a viewer who cannot edit', () => {
    server.use(
      http.post(`${API}/:entry/objects/:object/presign`, () =>
        HttpResponse.json({ url: 'https://bucket.test/x', expires_at: 'x' }),
      ),
    );
    render(
      <EntryTile
        guildId="g1"
        entry={entry()}
        channelName="general"
        onChanged={vi.fn()}
        onRemoved={vi.fn()}
      />,
    );
    expect(screen.queryByRole('button', { name: 'Remove' })).toBeNull();
  });
});

describe('Filters', () => {
  it('drops an emptied choice from the filter', async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    render(
      <Filters
        facets={{ tags: [{ tag: 'cats', count: 1 }], authors: [], channels: [] }}
        channelNames={{}}
        value={{ tag: 'cats' }}
        onChange={onChange}
      />,
    );
    await user.selectOptions(screen.getByLabelText('Tag'), '');
    expect(onChange).toHaveBeenCalledWith({});
  });
});

describe('ShelfPanel', () => {
  it('shows every failed bucket removal with its error and lets a manager retry or dismiss it', async () => {
    const removal = {
      id: 'r1',
      mount_id: 'mt',
      key: 'scrapbook/2026/06/42/0-cat.png',
      message_id: '42',
      attempts: 3,
      last_error: '403 AccessDenied: the key was revoked',
      next_attempt_at: 1,
      queued_at: 1,
    };
    let listed = [removal];
    const calls: string[] = [];
    server.use(
      http.get(`*/api/v1/guilds/g1/storage/mounts`, () =>
        HttpResponse.json([{ id: 'mt', label: 'Club shelf' }]),
      ),
      http.get(`${API}/status`, () =>
        HttpResponse.json(
          status({
            can_manage: true,
            failed_removals: listed.length,
            target: { mount_id: 'mt', label: 'Club shelf', granted_by: 'u1', ok: true },
          }),
        ),
      ),
      http.get(`${API}/removals`, () => HttpResponse.json({ removals: listed })),
      http.post(`${API}/removals/r1/retry`, () => {
        calls.push('retry');
        return HttpResponse.json({ removed: false });
      }),
      http.delete(`${API}/removals/r1`, () => {
        calls.push('dismiss');
        listed = [];
        return new HttpResponse(null, { status: 204 });
      }),
    );
    const user = userEvent.setup();
    render(<ShelfPanel guildId="g1" />);
    const list = await screen.findByTestId('scr-removals');
    expect(within(list).getByText('scrapbook/2026/06/42/0-cat.png')).toBeInTheDocument();
    expect(within(list).getByText(/AccessDenied/)).toBeInTheDocument();
    expect(await screen.findByTestId('scr-target')).toHaveTextContent('Club shelf');

    await user.click(within(list).getByRole('button', { name: 'Try again now' }));
    await waitFor(() => expect(calls).toEqual(['retry']));
    await user.click(await screen.findByRole('button', { name: 'I deleted it myself' }));
    await waitFor(() => expect(calls).toEqual(['retry', 'dismiss']));
    expect(await screen.findByTestId('scr-removals-none')).toBeInTheDocument();
  });

  it('is empty for a member who cannot manage the server', async () => {
    server.use(
      http.get(`${API}/status`, () => HttpResponse.json(status({ can_manage: false }))),
      http.get(`*/api/v1/guilds/g1/storage/mounts`, () => HttpResponse.json([])),
      http.get(`${API}/removals`, () => HttpResponse.json({ removals: [] })),
    );
    const { container } = render(<ShelfPanel guildId="g1" />);
    await waitFor(() => expect(useScrapbookStatus.getState().byGuild.g1).toBeTruthy());
    expect(container).toBeEmptyDOMElement();
  });

  it('RemovalList says so when nothing is stuck', () => {
    render(<RemovalList guildId="g1" removals={[]} onChanged={vi.fn()} />);
    expect(screen.getByTestId('scr-removals-none')).toBeInTheDocument();
  });
});

describe('ScrapbookBanner', () => {
  it('tells members that reactions archive messages, except in ignored channels or when off', async () => {
    server.use(
      http.get(`${API}/status`, () => HttpResponse.json(status({ ignored_channels: ['c9'] }))),
    );
    const { rerender } = render(<ScrapbookBanner guildId="g1" channelId="c1" />);
    const banner = await screen.findByTestId('scr-banner');
    expect(banner).toHaveTextContent('3 reactions');
    rerender(<ScrapbookBanner guildId="g1" channelId="c9" />);
    expect(screen.queryByTestId('scr-banner')).toBeNull();

    useScrapbookStatus.getState().reset();
    server.use(
      http.get(`${API}/status`, () =>
        HttpResponse.json(status({ offered: false, enabled: false })),
      ),
    );
    render(<ScrapbookBanner guildId="g1" channelId="c1" />);
    await waitFor(() => expect(useScrapbookStatus.getState().byGuild.g1).toBeTruthy());
    expect(screen.queryAllByTestId('scr-banner')).toHaveLength(0);
  });
});

function Probe({ guildId, pending = false }: { guildId: string; pending?: boolean }) {
  const onResult = vi.fn();
  const entry = useScrapbookMenuEntry(guildId, 'c1', 'm1', pending, onResult);
  return entry && 'label' in entry ? (
    <button type="button" onClick={() => 'onSelect' in entry && entry.onSelect?.()}>
      {entry.label}
    </button>
  ) : (
    <p>none</p>
  );
}

describe('"Send to Scrapbook" menu entry', () => {
  it('is offered to a permitted viewer and sends the message', async () => {
    let put = '';
    server.use(
      http.get(`${API}/status`, () => HttpResponse.json(status())),
      http.put('*/api/v1/channels/c1/messages/m1/scrapbook', ({ request }) => {
        put = request.url;
        return HttpResponse.json(entry());
      }),
    );
    const user = userEvent.setup();
    render(<Probe guildId="g1" />);
    await user.click(await screen.findByRole('button', { name: 'Send to Scrapbook' }));
    await waitFor(() => expect(put).toContain('/channels/c1/messages/m1/scrapbook'));
  });

  it.each([
    ['the operator has not offered it', status({ offered: false, enabled: false })],
    ['the server has it off', status({ enabled: false })],
    ['the viewer holds no permitted role', status({ can_send: false })],
  ])('is not offered when %s', async (_why, s) => {
    server.use(http.get(`${API}/status`, () => HttpResponse.json(s)));
    render(<Probe guildId="g1" />);
    await waitFor(() => expect(useScrapbookStatus.getState().byGuild.g1).toBeTruthy());
    expect(screen.getByText('none')).toBeInTheDocument();
  });

  it('is not offered for a message still being sent or outside a server', async () => {
    server.use(http.get(`${API}/status`, () => HttpResponse.json(status())));
    render(<Probe guildId="g1" pending />);
    await waitFor(() => expect(useScrapbookStatus.getState().byGuild.g1).toBeTruthy());
    expect(screen.getByText('none')).toBeInTheDocument();
    render(<Probe guildId="@me" />);
    expect(screen.getAllByText('none').length).toBeGreaterThan(0);
  });
});

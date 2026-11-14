import { createEvent, fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { vi } from 'vitest';
import { axeViolations } from '../../../../../../packages/ui/src/test-axe';
import { renderAt, phone } from '../../__tests__/harness';
import { MIN, addDays, startOfDay } from '../model';
import { NOW, guide, useTvApi } from './fixtures';

beforeEach(() => {
  // Fake the clock's date only: timers stay real so waitFor and user-event work.
  vi.useFakeTimers({ toFake: ['Date'], now: NOW });
  Element.prototype.scrollTo = vi.fn() as unknown as typeof Element.prototype.scrollTo;
});
afterEach(() => {
  vi.useRealTimers();
});

const cell = (name: RegExp | string) => screen.findByRole('button', { name });

describe('Live TV guide', () => {
  it('shows the page header, channels with numbers, cells and the now line; passes axe', async () => {
    useTvApi();
    const { container } = renderAt('/cinema/tv');
    expect(
      await screen.findByRole('heading', { level: 1, name: 'A familiar frequency.' }),
    ).toBeVisible();
    expect(screen.getByText('THE TELEVISION ROOM')).toBeInTheDocument();
    // Mockup demo labels are gone.
    expect(screen.queryByText(/SAMPLE LINEUP/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/No live stream connected/i)).not.toBeInTheDocument();

    const grid = await screen.findByRole('grid', { name: 'Programme guide' });
    expect(within(grid).getByText('101')).toBeInTheDocument();
    expect(within(grid).getByText('[adult swim]')).toBeInTheDocument();
    const now = await cell(/^Cowboy Bebop, S1 · E4 · Gateway Shuffle.*on now/);
    expect(now).toHaveTextContent('NOW');
    expect(now).toHaveTextContent('Anime · 1998');
    // Ratings are not in the schedule: they appear in the popover, from the catalog.
    // Age badge on the restricted film.
    expect(await cell(/^Alien,.*age-restricted/)).toBeInTheDocument();
    expect(screen.getByTestId('tv-nowline')).toBeInTheDocument();
    expect(
      screen.getByText(/Channels are assembled from the SIDE ROOM library\./),
    ).toBeInTheDocument();

    const found = await axeViolations(container);
    expect(found.map((v) => JSON.stringify(v).slice(0, 400))).toEqual([]);
  });

  it('shows the operator footer note as plain text, and the built-in one when there is none', async () => {
    const note = 'Times are local. <b>Bold</b> [a link](https://example.com) stays text.';
    useTvApi({ guide: { ...guide(), footer_note: note } });
    const { container } = renderAt('/cinema/tv');
    await screen.findByRole('grid');
    const foot = container.querySelector('.tv-footnote');
    expect(foot?.textContent).toBe(note);
    expect(foot?.querySelector('a, b')).toBeNull();
    expect(screen.queryByText(/Channels are assembled from the SIDE ROOM library./)).toBeNull();
  });

  it('wears the broadcast theme on this page only', async () => {
    useTvApi();
    const { container } = renderAt('/cinema/tv');
    await screen.findByRole('grid');
    const tv = container.querySelector('.tv-root');
    expect(tv?.getAttribute('data-theme')).toBe('sideroom-broadcast');
    expect(container.querySelector('.cin-root')?.getAttribute('data-theme')).toBe('sideroom');
  });

  it('fills the now panel with the first airing channel', async () => {
    useTvApi();
    renderAt('/cinema/tv');
    const panel = await screen.findByRole('region', { name: /On now, channel 101/ });
    expect(within(panel).getByRole('heading', { name: 'Cowboy Bebop' })).toBeInTheDocument();
    expect(within(panel).getByText('CHANNEL PREVIEW')).toBeInTheDocument();
    // The synopsis comes from the catalog, by title id.
    expect(
      await within(panel).findByText(/hunt bounties across the solar system/),
    ).toBeInTheDocument();
    expect(await within(panel).findByText('GOOD RECEPTION')).toBeInTheDocument();
    expect(within(panel).getByRole('link', { name: /Tune in/ })).toHaveAttribute(
      'href',
      '/cinema/tv/watch/c101',
    );
  });

  it('offers Watch together on the now panel and opens the party dialog for the channel', async () => {
    useTvApi();
    renderAt('/cinema/tv');
    const panel = await screen.findByRole('region', { name: /On now, channel 101/ });
    await userEvent.click(within(panel).getByRole('button', { name: /Watch together/ }));
    expect(await screen.findByText(/Start a party for Channel 101/)).toBeInTheDocument();
  });

  it('selecting another channel changes the preview', async () => {
    useTvApi();
    renderAt('/cinema/tv');
    const user = userEvent.setup();
    await screen.findByRole('grid');
    await user.click(screen.getByRole('button', { name: /Show channel 102 CARTOON NETWORK/ }));
    const panel = await screen.findByRole('region', { name: /On now, channel 102/ });
    expect(within(panel).getByRole('heading', { name: "Dexter's Laboratory" })).toBeInTheDocument();
  });

  it('filters by category and favourites', async () => {
    useTvApi({ favourites: ['c102'] });
    renderAt('/cinema/tv');
    const user = userEvent.setup();
    const grid = await screen.findByRole('grid');
    expect(within(grid).getAllByRole('row')).toHaveLength(5);
    await user.click(screen.getByRole('button', { name: 'Anime & animation' }));
    expect(within(screen.getByRole('grid')).getAllByRole('row')).toHaveLength(2);
    await user.click(screen.getByRole('button', { name: 'Movies' }));
    expect(within(screen.getByRole('grid')).getByText('105')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Custom' }));
    expect(within(screen.getByRole('grid')).getByText('106')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Favorites' }));
    await waitFor(() =>
      expect(within(screen.getByRole('grid')).getByText('102')).toBeInTheDocument(),
    );
    expect(within(screen.getByRole('grid')).queryByText('101')).not.toBeInTheDocument();
  });

  it('stores favourites on the server', async () => {
    const calls = useTvApi({ favourites: ['c102'] });
    renderAt('/cinema/tv');
    const user = userEvent.setup();
    await screen.findByRole('grid');
    const star = await screen.findByRole('button', { name: 'Add Adult Swim to favorites' });
    await user.click(star);
    await waitFor(() => expect(calls.favoriteCalls).toEqual(['PUT c101']));
    expect(calls.settingsPatches).toHaveLength(0);
    expect(
      await screen.findByRole('button', { name: 'Remove Adult Swim from favorites' }),
    ).toHaveAttribute('aria-pressed', 'true');
  });

  it('opens the programme popover with synopsis, next airings and links; Escape closes it', async () => {
    useTvApi();
    renderAt('/cinema/tv');
    const user = userEvent.setup();
    await user.click(await cell(/^Cowboy Bebop, S1 · E4/));
    const dialog = await screen.findByRole('dialog', { name: 'Cowboy Bebop' });
    expect(
      await within(dialog).findByText(/hunt bounties across the solar system/),
    ).toBeInTheDocument();
    expect(within(dialog).getByText('TV-14')).toBeInTheDocument();
    expect(within(dialog).getByText('S1 · E4 · Gateway Shuffle')).toBeInTheDocument();
    const airings = within(dialog).getByRole('region', { name: 'Next airings' });
    expect(within(airings).getAllByRole('listitem')).toHaveLength(1);
    expect(within(dialog).getByRole('link', { name: 'Tune in' })).toHaveAttribute(
      'href',
      '/cinema/tv/watch/c101',
    );
    expect(within(dialog).getByRole('link', { name: 'Go to title' })).toHaveAttribute(
      'href',
      '/cinema/t/3',
    );
    // On now: no reminder.
    expect(within(dialog).queryByRole('button', { name: 'Remind me' })).not.toBeInTheDocument();
    await user.keyboard('{Escape}');
    await waitFor(() =>
      expect(screen.queryByRole('dialog', { name: 'Cowboy Bebop' })).not.toBeInTheDocument(),
    );
  });

  it('offers Remind me for a later programme', async () => {
    const calls = useTvApi();
    renderAt('/cinema/tv');
    const user = userEvent.setup();
    await user.click(await cell(/^Samurai Champloo/));
    const dialog = await screen.findByRole('dialog', { name: 'Samurai Champloo' });
    expect(within(dialog).queryByRole('link', { name: 'Tune in' })).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Remind me' }));
    await waitFor(() => expect(calls.reminders).toEqual(['champloo']));
    expect(await within(dialog).findByRole('button', { name: 'Reminder set' })).toBeInTheDocument();
  });

  it('shows Reminder set after a reload and lists My reminders with Remove (H-01)', async () => {
    useTvApi({ reminders: ['champloo'] });
    renderAt('/cinema/tv');
    const user = userEvent.setup();
    const mine = await screen.findByRole('region', { name: 'My reminders' });
    expect(await within(mine).findByText('Reminder champloo')).toBeInTheDocument();
    await user.click(await cell(/^Samurai Champloo/));
    const dialog = await screen.findByRole('dialog', { name: 'Samurai Champloo' });
    expect(await within(dialog).findByRole('button', { name: 'Reminder set' })).toBeInTheDocument();
    await user.click(within(mine).getByRole('button', { name: 'Remove' }));
    expect(await within(mine).findByText('No reminders set.')).toBeInTheDocument();
  });

  it('saves the reminder lead into the settings blob (H-01)', async () => {
    useTvApi();
    const { server, http, HttpResponse } = await import('../../../test/server');
    const patches: unknown[] = [];
    server.use(
      http.get('*/api/v1/users/@me/settings', () =>
        HttpResponse.json({ version: 1, settings: { tv: { reminder_lead_minutes: 15 } } }),
      ),
      http.patch('*/api/v1/users/@me/settings', async ({ request }) => {
        const body = (await request.json()) as { settings: unknown };
        patches.push(body.settings);
        return HttpResponse.json({ version: 2, settings: body.settings });
      }),
    );
    renderAt('/cinema/tv');
    const mine = await screen.findByRole('region', { name: 'My reminders' });
    const select = within(mine).getByRole('combobox');
    await waitFor(() => expect(select).toHaveValue('15'));
    await userEvent.selectOptions(select, '30');
    await waitFor(() => expect(patches).toEqual([{ tv: { reminder_lead_minutes: 30 } }]));
  });

  it('says reminders are not available yet when the server has no such route', async () => {
    useTvApi();
    const { server, http } = await import('../../../test/server');
    server.use(http.put('*/api/v1/tv/reminders/:id', () => new Response(null, { status: 404 })));
    renderAt('/cinema/tv');
    const user = userEvent.setup();
    await user.click(await cell(/^Samurai Champloo/));
    await user.click(await screen.findByRole('button', { name: 'Remind me' }));
    expect(await screen.findByText('Reminders are not available yet.')).toBeInTheDocument();
  });

  it('moves focus across cells with the arrow keys and opens with Enter', async () => {
    useTvApi();
    renderAt('/cinema/tv');
    const user = userEvent.setup();
    const now = await cell(/^Cowboy Bebop, S1 · E4/);
    now.focus();
    await user.keyboard('{ArrowRight}');
    expect(await cell(/^Samurai Champloo/)).toHaveFocus();
    await user.keyboard('{ArrowLeft}{ArrowLeft}');
    expect(await cell(/^Earlier show/)).toHaveFocus();
    await user.keyboard('{ArrowRight}{ArrowDown}');
    // Row 2 at 20:00 is the hour-long Dexter block.
    expect(await cell(/^Dexter's Laboratory/)).toHaveFocus();
    await user.keyboard('{ArrowUp}');
    expect(await cell(/^Cowboy Bebop, S1 · E4/)).toHaveFocus();
    await user.keyboard('{Enter}');
    expect(await screen.findByRole('dialog', { name: 'Cowboy Bebop' })).toBeInTheDocument();
  });

  it('jumps to now with the n key and the Now button', async () => {
    useTvApi();
    renderAt('/cinema/tv');
    const user = userEvent.setup();
    await screen.findByRole('grid');
    const spy = Element.prototype.scrollTo as unknown as ReturnType<typeof vi.fn>;
    spy.mockClear();
    await user.keyboard('n');
    expect(spy).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole('button', { name: 'Now' }));
    expect(spy).toHaveBeenCalledTimes(2);
    const arg = spy.mock.calls[1]![0] as { left: number };
    // 20:15 into the day: 40.5 half-hour slots at 150 px minus half a slot.
    expect(arg.left).toBe(40.5 * 150 - 75);
  });

  it('navigates by day and refetches the guide for that day', async () => {
    const calls = useTvApi();
    renderAt('/cinema/tv');
    const user = userEvent.setup();
    await screen.findByRole('grid');
    expect(screen.getByText('Today')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Next day' }));
    expect(await screen.findByText('Tomorrow')).toBeInTheDocument();
    const tomorrow = addDays(startOfDay(NOW), 1);
    await waitFor(() => expect(calls.guide.some((q) => q.includes(`from=${tomorrow}`))).toBe(true));
    await user.click(screen.getByRole('button', { name: 'Previous day' }));
    await user.click(screen.getByRole('button', { name: 'Previous day' }));
    expect(await screen.findByText('Yesterday')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Previous day' })).toBeDisabled();
  });

  it('shows the lineup label and moves by a window with the arrows', async () => {
    useTvApi();
    renderAt('/cinema/tv');
    const user = userEvent.setup();
    await screen.findByRole('grid');
    expect(screen.getByText(/lineup$/)).toBeInTheDocument();
    const spy = Element.prototype.scrollTo as unknown as ReturnType<typeof vi.fn>;
    spy.mockClear();
    await user.click(screen.getByRole('button', { name: 'Later' }));
    expect((spy.mock.calls[0]![0] as { left: number }).left).toBe(40.5 * 150 - 75 + 4 * 150);
  });

  it('toggles channel time for the cell labels', async () => {
    useTvApi();
    renderAt('/cinema/tv');
    const user = userEvent.setup();
    const before = await cell(/^Dexter's Laboratory/);
    const localLabel = before.getAttribute('aria-label');
    await user.click(screen.getByRole('button', { name: 'Channel time' }));
    const after = await cell(/^Dexter's Laboratory/);
    // c102 is in UTC: the label changes unless the test machine is in UTC too.
    if (new Date(NOW).getTimezoneOffset() !== 0)
      expect(after.getAttribute('aria-label')).not.toBe(localLabel);
    expect(screen.getByRole('button', { name: 'Channel time' })).toHaveAttribute(
      'aria-pressed',
      'true',
    );
  });

  it('shows Create channel and Edit lineup only for MANAGE_TV staff', async () => {
    useTvApi();
    const first = renderAt('/cinema/tv');
    await screen.findByRole('grid');
    expect(screen.queryByRole('link', { name: '+ Create channel' })).not.toBeInTheDocument();
    expect(screen.queryByRole('link', { name: 'Edit lineup' })).not.toBeInTheDocument();
    first.unmount();

    useTvApi({ perms: String(1 << 7) });
    renderAt('/cinema/tv');
    expect(await screen.findByRole('link', { name: '+ Create channel' })).toHaveAttribute(
      'href',
      expect.stringContaining('/admin/cinema-live-tv'),
    );
    expect(screen.getByRole('link', { name: 'Edit lineup' })).toBeInTheDocument();
  });

  it('says Live TV is not available yet when the server has no routes', async () => {
    useTvApi({ guideStatus: 404 });
    renderAt('/cinema/tv');
    expect(await screen.findByText('Live TV is not available yet')).toBeInTheDocument();
  });

  it('shows a retryable error for a server failure', async () => {
    useTvApi({ guideStatus: 500 });
    renderAt('/cinema/tv');
    expect(await screen.findByText('We could not load the guide.')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument();
  });

  it('shows an empty state when there are no channels', async () => {
    useTvApi({ guide: { ...guide(), channels: [], blocks: {} } });
    renderAt('/cinema/tv');
    expect(await screen.findByText('No channels yet')).toBeInTheDocument();
  });

  it('has a Live TV nav entry pointing at the guide', async () => {
    useTvApi();
    renderAt('/cinema/tv');
    await screen.findByRole('grid');
    const nav = screen.getByRole('navigation', { name: 'Cinema' });
    const link = within(nav).getByRole('link', { name: /Live TV/ });
    expect(link).toHaveAttribute('href', '/cinema/tv');
    expect(link).toHaveAttribute('aria-current', 'page');
  });
});

describe('Live TV guide on a phone', () => {
  let restore: () => void;
  beforeEach(() => {
    restore = phone();
  });
  afterEach(() => restore());

  it('lists an agenda per channel and opens the programme as a dialog; passes axe', async () => {
    useTvApi();
    const { container } = renderAt('/cinema/tv');
    const user = userEvent.setup();
    const list = await screen.findByRole('list', { name: 'Channels' });
    expect(screen.queryByRole('grid')).not.toBeInTheDocument();
    const adult = within(list).getByRole('list', { name: 'Adult Swim schedule' });
    // The programme on now comes first, then what is coming; finished ones are not listed.
    const items = within(adult).getAllByRole('button');
    expect(items[0]).toHaveTextContent('NOW');
    expect(items[0]).toHaveTextContent('Cowboy Bebop');
    expect(within(adult).queryByText('Earlier show')).not.toBeInTheDocument();
    expect(await axeViolations(container)).toEqual([]);
    await user.click(items[0]!);
    expect(await screen.findByRole('dialog', { name: 'Cowboy Bebop' })).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
  });

  it("shows the operator's footer note on the phone, and the built-in text when there is none", async () => {
    useTvApi({ guide: { ...guide(), footer_note: 'Phone note: all times are local.' } });
    const { container, unmount } = renderAt('/cinema/tv');
    await screen.findByRole('list', { name: 'Channels' });
    expect(container.querySelector('.tv-footnote')?.textContent).toBe(
      'Phone note: all times are local.',
    );
    unmount();
    useTvApi();
    const again = renderAt('/cinema/tv');
    await screen.findByRole('list', { name: 'Channels' });
    expect(again.container.querySelector('.tv-footnote')?.textContent).toMatch(
      /Channels are assembled from the SIDE ROOM library\./,
    );
  });

  describe('swiping between channels', () => {
    // jsdom has no PointerEvent: a MouseEvent that also carries the pointer id and type.
    beforeEach(() => {
      class TestPointerEvent extends MouseEvent {
        pointerId: number;
        pointerType: string;
        constructor(type: string, init: PointerEventInit = {}) {
          super(type, init);
          this.pointerId = init.pointerId ?? 0;
          this.pointerType = init.pointerType ?? '';
        }
      }
      vi.stubGlobal('PointerEvent', TestPointerEvent);
    });
    afterEach(() => {
      vi.unstubAllGlobals();
    });
    const touch = { pointerId: 1, pointerType: 'touch' };
    const swipe = (el: Element, dx: number, dy = 0, type = 'touch') => {
      fireEvent.pointerDown(el, { pointerId: 1, pointerType: type, clientX: 200, clientY: 300 });
      fireEvent.pointerUp(el, {
        pointerId: 1,
        pointerType: type,
        clientX: 200 + dx,
        clientY: 300 + dy,
      });
    };
    const panel = () => screen.getByRole('region', { name: /^On now, channel/ });

    it('a swipe of at least 48 px, more sideways than vertical, selects the next or previous channel', async () => {
      useTvApi();
      const scrollIntoView = vi.fn();
      Element.prototype.scrollIntoView = scrollIntoView;
      renderAt('/cinema/tv');
      const list = await screen.findByRole('list', { name: 'Channels' });
      expect(panel()).toHaveAccessibleName('On now, channel 101');
      // Fingers move left: the next channel.
      swipe(list, -120, 10);
      await waitFor(() => expect(panel()).toHaveAccessibleName('On now, channel 102'));
      expect(screen.getByRole('status', { name: '' })).toHaveTextContent(
        'Showing channel 102, Cartoon Network.',
      );
      expect(list.querySelector('[data-selected]')).toHaveTextContent('Cartoon Network');
      expect(scrollIntoView).toHaveBeenCalledTimes(1);
      expect(scrollIntoView).toHaveBeenCalledWith({ block: 'start', behavior: 'smooth' });
      // Fingers move right: the previous one; at the first channel nothing more happens.
      swipe(list, 90, -20);
      await waitFor(() => expect(panel()).toHaveAccessibleName('On now, channel 101'));
      swipe(list, 150);
      expect(panel()).toHaveAccessibleName('On now, channel 101');
      expect(scrollIntoView).toHaveBeenCalledTimes(2);
      void touch;
    });

    it('a short move, a mostly vertical one and a mouse drag do nothing', async () => {
      useTvApi();
      renderAt('/cinema/tv');
      const list = await screen.findByRole('list', { name: 'Channels' });
      swipe(list, -47, 0);
      swipe(list, -70, 120);
      swipe(list, -70, -70);
      swipe(list, -200, 0, 'mouse');
      fireEvent.pointerDown(list, {
        pointerId: 1,
        pointerType: 'touch',
        clientX: 200,
        clientY: 300,
      });
      fireEvent.pointerCancel(list, { pointerId: 1, pointerType: 'touch' });
      fireEvent.pointerUp(list, { pointerId: 1, pointerType: 'touch', clientX: 20, clientY: 300 });
      expect(panel()).toHaveAccessibleName('On now, channel 101');
    });

    it('never blocks vertical scrolling and does not animate when the viewer asked for less motion', async () => {
      useTvApi();
      const base = window.matchMedia;
      const phoneMatch = base;
      window.matchMedia = ((q: string) => ({
        matches: q.includes('max-width') || q.includes('prefers-reduced-motion'),
        media: q,
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
      })) as unknown as typeof window.matchMedia;
      const scrollIntoView = vi.fn();
      Element.prototype.scrollIntoView = scrollIntoView;
      renderAt('/cinema/tv');
      const list = await screen.findByRole('list', { name: 'Channels' });
      // Nothing is prevented: the touch gesture is not cancelled by the handlers.
      const down = createEvent.pointerDown(list, {
        pointerId: 1,
        pointerType: 'touch',
        clientX: 200,
        clientY: 300,
      });
      fireEvent(list, down);
      const up = createEvent.pointerUp(list, {
        pointerId: 1,
        pointerType: 'touch',
        clientX: 90,
        clientY: 305,
      });
      fireEvent(list, up);
      expect(down.defaultPrevented).toBe(false);
      expect(up.defaultPrevented).toBe(false);
      await waitFor(() => expect(panel()).toHaveAccessibleName('On now, channel 102'));
      expect(scrollIntoView).toHaveBeenCalledWith({ block: 'start', behavior: 'auto' });
      window.matchMedia = phoneMatch;
    });
  });
});

it('keeps MIN exported for the model', () => {
  expect(MIN).toBe(60_000);
});

// D-466: the shelf picker dialog an Activity opens with pickShelfObject: it lists the member's shelves and the files
// of one, hides what `accept` and `max_size` rule out, and returns one object (or null on cancel).
import { ThemeProvider } from '@sideroom/ui';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { ShelfObjectPick } from '@sideroom/embedded-app-sdk';

const H1 = 'a'.repeat(64);
const H2 = 'b'.repeat(64);
const H3 = 'c'.repeat(64);

const visibleShelves = vi.fn();
const shelfCatalogPage = vi.fn();
vi.mock('../../study/api', () => ({ visibleShelves: () => visibleShelves() }));
vi.mock('../api', () => ({ shelfCatalogPage: (...a: unknown[]) => shelfCatalogPage(...a) }));

import ShelfPicker from '../ShelfPicker';

const obj = (object_hash: string, path: string, size: number) => ({
  object_hash,
  path,
  size,
  title: '',
});

beforeEach(() => {
  visibleShelves
    .mockReset()
    .mockResolvedValue([{ shelf_id: '55', kind: 'own', label: 'Home bucket' }]);
  shelfCatalogPage.mockReset().mockResolvedValue({
    objects: [
      obj(H1, 'games/pack.json', 900),
      obj(H2, 'pictures/cat.png', 500),
      obj(H3, 'games/huge.json', 5_000_000),
    ],
    next_cursor: null,
  });
});

const show = (
  onDone: (p: ShelfObjectPick | null) => void,
  req = { accept: ['.json'], maxSize: 65536 as number | null },
) =>
  render(
    <ThemeProvider section="lounge">
      <ShelfPicker appName="Sketch & Slander" req={req} onDone={onDone} />
    </ThemeProvider>,
  );

describe('ShelfPicker (D-466)', () => {
  it('states the cap when the 15th page still has a next page (D-537)', async () => {
    let n = 0;
    shelfCatalogPage.mockReset().mockImplementation(async () => {
      n += 1;
      return {
        objects: [obj(String(n).padStart(64, '0'), `f${n}.json`, 10)],
        next_cursor: `c${n}`,
      };
    });
    show(vi.fn());
    expect(
      await screen.findByText(
        'This shelf has more than 3,000 files; only the first 3,000 are listed.',
      ),
    ).toBeTruthy();
    expect(shelfCatalogPage).toHaveBeenCalledTimes(15);
  });

  it('says nothing when the shelf ends inside the cap', async () => {
    show(vi.fn());
    await screen.findByText('pack.json');
    expect(screen.queryByText(/more than 3,000/)).toBeNull();
  });

  it('lists only the files the game can use, and says how many are hidden', async () => {
    show(vi.fn());
    expect(await screen.findByText('pack.json')).toBeTruthy();
    expect(screen.queryByText('cat.png')).toBeNull();
    expect(screen.queryByText('huge.json')).toBeNull();
    expect(screen.getByText(/2 files are hidden/)).toBeTruthy();
    expect(screen.getByRole('dialog', { name: 'Choose a file for Sketch & Slander' })).toBeTruthy();
  });

  it('returns the one chosen object with its name, size and media type', async () => {
    const done = vi.fn();
    show(done);
    await userEvent.click(await screen.findByRole('button', { name: /pack\.json/ }));
    expect(done).toHaveBeenCalledTimes(1);
    expect(done).toHaveBeenCalledWith({
      shelf_id: '55',
      object_hash: H1,
      name: 'pack.json',
      size: 900,
      content_type: 'application/json',
    });
  });

  it('cancel returns null', async () => {
    const done = vi.fn();
    show(done);
    await screen.findByText('pack.json');
    await userEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(done).toHaveBeenCalledWith(null);
  });

  it('narrows by search and says so when nothing matches', async () => {
    show(vi.fn(), { accept: [], maxSize: null });
    await screen.findByText('pack.json');
    await userEvent.type(screen.getByRole('textbox', { name: 'Search files' }), 'cat');
    expect(screen.queryByText('pack.json')).toBeNull();
    expect(screen.getByText('cat.png')).toBeTruthy();
    await userEvent.type(screen.getByRole('textbox', { name: 'Search files' }), 'zzz');
    expect(await screen.findByText('No file matches that search.')).toBeTruthy();
  });

  it('P51: asks the server to search once typing pauses, and starts over from page one', async () => {
    show(vi.fn(), { accept: [], maxSize: null });
    await screen.findByText('pack.json');
    expect(shelfCatalogPage).toHaveBeenLastCalledWith('55', null, undefined);
    shelfCatalogPage.mockClear();
    shelfCatalogPage.mockResolvedValue({
      objects: [obj(H2, 'pictures/cat.png', 500)],
      next_cursor: null,
    });
    await userEvent.type(screen.getByRole('textbox', { name: 'Search files' }), 'cat');
    await waitFor(() => expect(shelfCatalogPage).toHaveBeenCalledWith('55', null, 'cat'));
    // One request for the finished word, not one per key.
    expect(shelfCatalogPage.mock.calls.filter((c) => c[2] === 'ca' || c[2] === 'c')).toHaveLength(
      0,
    );
    expect(await screen.findByText('cat.png')).toBeTruthy();
  });

  it('P51: a search that still has more pages says so, with its own note', async () => {
    show(vi.fn(), { accept: [], maxSize: null });
    await screen.findByText('pack.json');
    let n = 0;
    shelfCatalogPage.mockReset().mockImplementation(async () => {
      n += 1;
      return {
        objects: [obj(String(n).padStart(64, '0'), `s${n}.json`, 10)],
        next_cursor: `c${n}`,
      };
    });
    await userEvent.type(screen.getByRole('textbox', { name: 'Search files' }), 's');
    expect(await screen.findByText(/More than 3,000 files match/)).toBeTruthy();
    expect(screen.queryByText(/This shelf has more than/)).toBeNull();
  });

  it('says so when there are no shelves, and when a shelf cannot be read', async () => {
    visibleShelves.mockResolvedValue([]);
    const a = show(vi.fn());
    expect(await screen.findByText('You have no shelves to choose from yet.')).toBeTruthy();
    a.unmount();
    visibleShelves.mockResolvedValue([{ shelf_id: '55', kind: 'own', label: 'Home bucket' }]);
    shelfCatalogPage.mockRejectedValue(new Error('403'));
    show(vi.fn());
    await waitFor(() => expect(screen.getByRole('alert').textContent).toMatch(/could not be read/));
  });
});

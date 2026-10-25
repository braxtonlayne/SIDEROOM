import { expect, test } from '../support/world';
import {
  LINKS_NOT_TABBABLE,
  focusWithin,
  isFocused,
  tabTo,
  tabsToLinks,
  watchText,
  textHistory,
} from '../support/keys';

/**
 * RDY-A11Y-003, flow 7: the Library reader with the keyboard only. Arrows and End turn pages, the reader's
 * status line says "Page 2 of 3", the end card offers the next chapter (Enter opens it and the status starts
 * again), the settings drawer opens from its button, traps focus, closes with Escape and gives focus back.
 *
 * The chapter data is served by the spec itself (page.route on the reader's manifest, series and chapter list,
 * and a one-pixel PNG for every page image), so the flow runs the same on every browser without object storage.
 * Everything else the reader asks the server for goes to the real server.
 */
const PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==',
  'base64',
);

const chapter = (n: number) => ({
  id: `kb${n}`,
  series_id: 'kb1',
  edition_id: 'e1',
  number: n,
  title: null,
  page_count: 3,
  format: 'images',
  published: true,
  published_at_ms: 1_700_000_000_000,
  release_group: 'Keyboard',
});

const manifest = (n: number) => ({
  chapter: chapter(n),
  reading_direction: 'ltr',
  pages: [0, 1, 2].map((i) => ({
    index: i,
    width: 800,
    height: 1200,
    is_double_spread: false,
    url: `/cdn/kb-${n}-${i}.png`,
    variants: [],
    blurhash: null,
  })),
  expires_at: 9_999_999_999,
  next_chapter_id: n < 2 ? `kb${n + 1}` : null,
  prev_chapter_id: n > 1 ? `kb${n - 1}` : null,
});

test('reader: turn pages, change chapter, open and close the settings, keyboard only', async ({
  solo,
}) => {
  const { a } = solo;
  const page = a.page;
  test.skip(!(await tabsToLinks(page)), LINKS_NOT_TABBABLE);
  await page.route('**/cdn/kb-*.png', (route) =>
    route.fulfill({ contentType: 'image/png', body: PNG }),
  );
  await page.route(/\/api\/v1\/library\/chapters\/kb(\d)\/pages$/, (route) => {
    const n = Number(/kb(\d)\/pages/.exec(route.request().url())?.[1]);
    return route.fulfill({ json: manifest(n) });
  });
  await page.route('**/api/v1/library/series/kb1', (route) =>
    route.fulfill({
      json: {
        series: {
          id: 'kb1',
          kind: 'manga',
          title: 'Keyboard Tales',
          alt_titles: [],
          authors: [],
          artists: [],
          status: 'ongoing',
          synopsis: '',
          genres: [],
          tags: [],
          reading_direction: 'ltr',
          age_restricted: false,
          published: true,
          chapter_count: 2,
          follower_count: 0,
          cover_url: '/cdn/kb-cover.png',
          created_at_ms: 1,
          updated_at_ms: 2,
        },
        editions: [
          { id: 'e1', series_id: 'kb1', language: 'en', label: 'Official', is_default: true },
        ],
        follow: null,
      },
    }),
  );
  await page.route('**/api/v1/library/series/kb1/chapters?*', (route) =>
    route.fulfill({ json: { editions: [], volumes: [], chapters: [chapter(1), chapter(2)] } }),
  );
  await watchText(page, '[data-testid="reader-status"]', '__status');

  await page.goto('/library/r/kb1');
  const status = page.getByTestId('reader-status');
  await expect(status).toHaveText('Page 1 of 3');

  // Pages: the arrow keys and End turn them; the status line (a polite live region) says where we are.
  await expect(status).toHaveAttribute('aria-live', 'polite');
  await page.keyboard.press('ArrowRight');
  await expect(status).toHaveText('Page 2 of 3');
  await expect(page).toHaveURL(/\/library\/r\/kb1\/2/);
  await page.keyboard.press('ArrowLeft');
  await expect(status).toHaveText('Page 1 of 3');
  await page.keyboard.press('End');
  await expect(status).toHaveText('Page 3 of 3');
  await page.keyboard.press('ArrowRight');
  await expect(status).toHaveText('End of chapter');
  const end = page.getByRole('region', { name: 'End of chapter' });
  await expect(end).toBeVisible();

  // Chapter change: Tab to "Next chapter" in the end card, Enter opens it, the status starts again.
  const next = end.getByRole('button', { name: /Next chapter/ });
  await tabTo(page, next, { max: 30 });
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/\/library\/r\/kb2/);
  await expect.poll(() => textHistory(page, '__status')).toContain('Page 1 of 3');
  await expect(status).toHaveText('Page 1 of 3');

  // Settings drawer: opens from its button, traps focus, Escape closes it, focus returns to the button.
  const settings = page.getByRole('button', { name: 'Reader settings' });
  await tabTo(page, settings, { max: 30 });
  await page.keyboard.press('Enter');
  const drawer = page.getByRole('dialog');
  await expect(drawer).toBeVisible();
  await expect.poll(() => focusWithin(page, drawer)).toBe(true);
  for (let i = 0; i < 10; i++) {
    await page.keyboard.press('Tab');
    expect(await focusWithin(page, drawer), `Tab ${i + 1} left the drawer`).toBe(true);
  }
  await page.keyboard.press('Escape');
  await expect(drawer).toBeHidden();
  await expect.poll(() => isFocused(settings)).toBe(true);
  // The reader is still on chapter 2 (Escape closed the drawer, not the reader).
  await expect(page).toHaveURL(/\/library\/r\/kb2/);
});

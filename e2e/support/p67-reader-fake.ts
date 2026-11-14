import { deflateSync } from 'node:zlib';
import type { BrowserContext, Page } from '@playwright/test';

/**
 * Wave 67 (reader): a one-chapter Library that lives in the test. A fresh stack has no readable chapter without an
 * upload and a conversion, so the series, the chapter, the page images and the progress routes (with the server's
 * conflict rule: a save whose `base_updated_at_ms` is older than the stored position is not written) are answered
 * by `page.route`. The gateway frame is dispatched into the page through the app's own event seam.
 */
export interface FakeServer {
  progress: {
    page_index: number;
    percent: number;
    completed: boolean;
    updated_at_ms: number;
  } | null;
  clock: number;
  puts: { page_index: number; base: number | null; applied: boolean; at: number }[];
}

export const PAGES = 8;
export const newFake = (): FakeServer => ({ progress: null, clock: 1_800_000_000_000, puts: [] });

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc = (b: Buffer) => {
  let c = 0xffffffff;
  for (const x of b) c = crcTable[(c ^ x) & 0xff]! ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};
const chunk = (type: string, data: Buffer) => {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const sum = Buffer.alloc(4);
  sum.writeUInt32BE(crc(body));
  return Buffer.concat([len, body, sum]);
};
/** A 200x300 PNG: a coloured page with the page number drawn as a block of bars, so pages differ at a glance. */
function pagePng(n: number): Buffer {
  const w = 200;
  const h = 300;
  const rows: Buffer[] = [];
  const hue = [(60 + n * 25) % 255, (140 + n * 40) % 255, (200 - n * 15 + 255) % 255];
  for (let y = 0; y < h; y++) {
    const row = Buffer.alloc(1 + w * 3);
    for (let x = 0; x < w; x++) {
      const bar = y > 120 && y < 180 && Math.floor(x / 20) < n + 1 && x % 20 < 14;
      const o = 1 + x * 3;
      row[o] = bar ? 250 : hue[0]!;
      row[o + 1] = bar ? 250 : hue[1]!;
      row[o + 2] = bar ? 250 : hue[2]!;
    }
    rows.push(row);
  }
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(Buffer.concat(rows))),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

const SERIES = {
  id: '9001',
  kind: 'manga',
  title: 'Frieren: Beyond Journey',
  alt_titles: [],
  authors: ['An Author'],
  artists: [],
  status: 'ongoing',
  synopsis: 'A long walk after the end of the quest. A second sentence.',
  genres: ['fantasy'],
  tags: [],
  reading_direction: 'ltr',
  age_restricted: false,
  published: true,
  chapter_count: 1,
  follower_count: 10,
  cover_url: '/cdn/p67-cover.png',
  created_at_ms: 1,
  updated_at_ms: 2,
};
const chapter = (fake: FakeServer) => ({
  id: '9101',
  series_id: SERIES.id,
  edition_id: '9201',
  number: 36,
  title: null,
  page_count: PAGES,
  format: 'images',
  published: true,
  published_at_ms: 1_700_000_000_000,
  release_group: 'Group X',
  progress: fake.progress,
});

export const CHAPTER_ID = '9101';
export const READER_PATH = `/library/r/${CHAPTER_ID}/1`;

/** Install the fake Library on a context (every page of it). */
export async function installFakeLibrary(ctx: BrowserContext, fake: FakeServer) {
  const json = (r: import('@playwright/test').Route, body: unknown, status = 200) =>
    r.fulfill({ status, json: body });
  await ctx.route('**/cdn/p67-*.png', (r) => {
    const m = /p67-(?:p(\d+)|cover)\.png/.exec(r.request().url());
    return r.fulfill({
      status: 200,
      contentType: 'image/png',
      body: pagePng(m?.[1] ? Number(m[1]) : 0),
    });
  });
  await ctx.route(/\/api\/v1\/library\/series\/9001(\?.*)?$/, (r) =>
    json(r, {
      series: SERIES,
      editions: [
        { id: '9201', series_id: SERIES.id, language: 'en', label: 'Official', is_default: true },
      ],
      follow: null,
    }),
  );
  await ctx.route(/\/api\/v1\/library\/series\/9001\/chapters(\?.*)?$/, (r) =>
    json(r, { editions: [], volumes: [], chapters: [chapter(fake)] }),
  );
  await ctx.route(/\/api\/v1\/library\/chapters\/9101\/pages(\?.*)?$/, (r) =>
    json(r, {
      chapter: chapter(fake),
      reading_direction: 'ltr',
      pages: Array.from({ length: PAGES }, (_, i) => ({
        index: i,
        width: 200,
        height: 300,
        is_double_spread: false,
        url: `/cdn/p67-p${i}.png`,
        variants: [],
        blurhash: null,
      })),
      expires_at: 9_999_999_999,
      next_chapter_id: null,
      prev_chapter_id: null,
    }),
  );
  await ctx.route(/\/api\/v1\/library\/chapters\/9101\/(bookmarks|comments)(\?.*)?$/, (r) =>
    json(
      r,
      r.request().url().includes('bookmarks')
        ? { bookmarks: [] }
        : { items: [], next_before: null, total: 0 },
    ),
  );
  await ctx.route(/\/api\/v1\/library\/chapters\/9101\/progress(\?.*)?$/, async (r) => {
    const req = r.request();
    if (req.method() === 'GET')
      return fake.progress ? json(r, fake.progress) : json(r, { message: 'Not found' }, 404);
    if (req.method() === 'PUT') {
      const b = req.postDataJSON() as {
        page_index: number;
        percent: number;
        completed: boolean;
        base_updated_at_ms?: number;
      };
      const base = b.base_updated_at_ms ?? null;
      const conflict =
        base !== null && fake.progress !== null && fake.progress.updated_at_ms > base;
      if (!conflict)
        fake.progress = {
          page_index: b.page_index,
          percent: b.percent,
          completed: b.completed,
          updated_at_ms: ++fake.clock,
        };
      fake.puts.push({ page_index: b.page_index, base, applied: !conflict, at: Date.now() });
      return json(r, { progress: fake.progress, applied: !conflict });
    }
    return json(r, {}, 204);
  });
  await ctx.route(/\/api\/v1\/library\/me\/progress(\?.*)?$/, (r) =>
    json(r, {
      items: fake.progress ? [{ series: SERIES, chapter: chapter(fake) }] : [],
      next_before: null,
    }),
  );
}

/** What the gateway sends the account's other sessions when a position is saved (docs/05 section 4). */
export const progressFrame = (fake: FakeServer) => ({
  type: 'progress',
  chapter_id: CHAPTER_ID,
  series_id: SERIES.id,
  page_index: fake.progress!.page_index,
  percent: fake.progress!.percent,
  cfi: null,
  completed: fake.progress!.completed,
  updated_at_ms: fake.progress!.updated_at_ms,
});

/** Deliver a gateway dispatch to a page through the app's own event seam (dev server only). */
export async function dispatch(page: Page, event: string, data: unknown) {
  await page.evaluate(
    async ([e, d]) => {
      const url = '/src/store/events.ts';
      const m = await import(/* @vite-ignore */ url);
      (m as { emitDispatch(e: string, d: unknown): void }).emitDispatch(e as string, d);
    },
    [event, data] as const,
  );
}

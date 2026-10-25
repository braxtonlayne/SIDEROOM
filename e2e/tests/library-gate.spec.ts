import type { APIRequestContext, Page } from '@playwright/test';
import { deflateSync } from 'node:zlib';
import { apiPost, expect, test, type SignedIn } from '../support/fixtures';

/**
 * RDY-LIB-001, the Library exit gate (D-368, T-02), on all three browsers:
 *
 *  1. A member starts a new RTL series submission and sends ten chapters, each a ZIP built here from generated
 *     PNG pages (no fixture files). Chapter 1 goes through the browser (add a chapter, choose the file, the
 *     in-browser archive check, the presigned PUT); chapters 2 to 10 use the same routes the page calls.
 *  2. A reviewer (the Owner) claims it, approves nine chapters and rejects one with a note, then asks for changes
 *     with a reason. The member opens the submission and sees the reason and the note.
 *  3. The member replaces the rejected chapter's file and sends it again; the reviewer approves it and publishes.
 *  4. A reader opens the RTL chapter at a phone-sized viewport (the arrow keys follow the right-to-left order), and a
 *     second browser context signed in as the same member sees the page position move.
 *
 * The uploads are presigned PUTs to object storage and the chapters are converted by the ingest job, so the flow
 * runs in the `live` CI job (SR_E2E_LIVE=1: the real Garage stack, SR_ADMIN_REQUIRE_REAUTH=0). The generated ZIPs
 * are checked by a test that needs no storage and runs everywhere. The device half of the gate (a real phone, a
 * real screen reader) stays in docs/dev/owner-checklist.md.
 */

const API = '/api/v1';
const CHAPTERS = 10;
// Four, not three: the follow-along ends on page 3, and the last page marks the chapter read (Reader.tsx
// `completed: i >= count - 1`), so a fresh visit to a read chapter rightly opens at page 1 (the third CI run).
const PAGES = 4;
/** Chapter 10 is the one the reviewer sends back. */
const SENT_BACK = CHAPTERS;
const PHONE = { width: 390, height: 844 };

// ---- generated pages and ZIPs -----------------------------------------------------------------------------

const CRC_TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

function crc32(buf: Uint8Array): number {
  let c = 0xffffffff;
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff]! ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

const u32be = (n: number) => {
  const b = Buffer.alloc(4);
  b.writeUInt32BE(n >>> 0, 0);
  return b;
};

function pngChunk(type: string, data: Buffer): Buffer {
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  return Buffer.concat([u32be(data.length), body, u32be(crc32(body))]);
}

/** A small truecolour PNG whose colours depend on `seed`, so every page of every chapter differs. */
function generatedPage(seed: number, width = 160, height = 240): Buffer {
  const rows: Buffer[] = [];
  for (let y = 0; y < height; y++) {
    const row = Buffer.alloc(1 + width * 3); // filter byte 0
    for (let x = 0; x < width; x++) {
      row[1 + x * 3] = (seed * 37 + x) & 0xff;
      row[2 + x * 3] = (seed * 91 + y) & 0xff;
      row[3 + x * 3] = (seed * 53 + ((x + y) >> 1)) & 0xff;
    }
    rows.push(row);
  }
  const ihdr = Buffer.concat([u32be(width), u32be(height), Buffer.from([8, 2, 0, 0, 0])]);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    pngChunk('IHDR', ihdr),
    pngChunk('IDAT', deflateSync(Buffer.concat(rows))),
    pngChunk('IEND', Buffer.alloc(0)),
  ]);
}

/** A ZIP with the entries stored uncompressed (PNG is compressed already). */
function zipOf(entries: { name: string; data: Buffer }[]): Buffer {
  const locals: Buffer[] = [];
  const centrals: Buffer[] = [];
  let offset = 0;
  const le16 = (n: number) => {
    const b = Buffer.alloc(2);
    b.writeUInt16LE(n, 0);
    return b;
  };
  const le32 = (n: number) => {
    const b = Buffer.alloc(4);
    b.writeUInt32LE(n >>> 0, 0);
    return b;
  };
  for (const e of entries) {
    const name = Buffer.from(e.name, 'utf8');
    const crc = crc32(e.data);
    const common = [
      le16(20), // version needed
      le16(0x0800), // UTF-8 names
      le16(0), // stored
      le16(0), // time
      le16(0x21), // date: 1980-01-01
      le32(crc),
      le32(e.data.length),
      le32(e.data.length),
      le16(name.length),
      le16(0), // extra length
    ];
    const local = Buffer.concat([le32(0x04034b50), ...common, name, e.data]);
    centrals.push(
      Buffer.concat([
        le32(0x02014b50),
        le16(20), // version made by
        ...common,
        le16(0), // comment length
        le16(0), // disk
        le16(0), // internal attributes
        le32(0), // external attributes
        le32(offset),
        name,
      ]),
    );
    locals.push(local);
    offset += local.length;
  }
  const central = Buffer.concat(centrals);
  const end = Buffer.concat([
    le32(0x06054b50),
    le16(0),
    le16(0),
    le16(entries.length),
    le16(entries.length),
    le32(central.length),
    le32(offset),
    le16(0),
  ]);
  return Buffer.concat([...locals, central, end]);
}

/** Chapter `n` as a ZIP of `001.png`, `002.png`, ... `version` changes every page (a replaced file). */
function chapterZip(n: number, version = 0): Buffer {
  return zipOf(
    Array.from({ length: PAGES }, (_, i) => ({
      name: `${String(i + 1).padStart(3, '0')}.png`,
      data: generatedPage(n * 10 + i + version * 1000),
    })),
  );
}

/** Reads a ZIP back (stored entries only): names, bytes, and whether each CRC matches. */
function readZip(zip: Buffer): { name: string; data: Buffer; crcOk: boolean }[] {
  const eocd = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  if (eocd < 0) throw new Error('no end of central directory');
  const count = zip.readUInt16LE(eocd + 10);
  let p = zip.readUInt32LE(eocd + 16);
  const out: { name: string; data: Buffer; crcOk: boolean }[] = [];
  for (let i = 0; i < count; i++) {
    if (zip.readUInt32LE(p) !== 0x02014b50) throw new Error('bad central header');
    const crc = zip.readUInt32LE(p + 16);
    const size = zip.readUInt32LE(p + 24);
    const nameLen = zip.readUInt16LE(p + 28);
    const extraLen = zip.readUInt16LE(p + 30);
    const commentLen = zip.readUInt16LE(p + 32);
    const local = zip.readUInt32LE(p + 42);
    const name = zip.subarray(p + 46, p + 46 + nameLen).toString('utf8');
    const dataStart = local + 30 + zip.readUInt16LE(local + 26) + zip.readUInt16LE(local + 28);
    const data = zip.subarray(dataStart, dataStart + size);
    out.push({ name, data, crcOk: crc32(data) === crc });
    p += 46 + nameLen + extraLen + commentLen;
  }
  return out;
}

test('the generated chapter ZIPs are well-formed archives of distinct PNG pages', () => {
  const seen = new Set<string>();
  for (let n = 1; n <= CHAPTERS; n++) {
    const files = readZip(chapterZip(n));
    expect(files.map((f) => f.name)).toEqual(['001.png', '002.png', '003.png']);
    for (const f of files) {
      expect(f.crcOk, `${n}/${f.name} CRC`).toBe(true);
      expect(f.data.subarray(0, 8)).toEqual(
        Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
      );
      expect(f.data.readUInt32BE(16)).toBe(160); // IHDR width
      expect(f.data.readUInt32BE(20)).toBe(240); // IHDR height
      seen.add(f.data.toString('base64'));
    }
  }
  expect(seen.size).toBe(CHAPTERS * PAGES);
  // A replaced file is different bytes.
  expect(chapterZip(SENT_BACK, 1).equals(chapterZip(SENT_BACK, 0))).toBe(false);
});

// ---- the gate --------------------------------------------------------------------------------------------

test.describe('Library exit gate', () => {
  test.skip(
    process.env.SR_E2E_LIVE !== '1',
    'Uploads are presigned PUTs to object storage and the ingest job converts them; set SR_E2E_LIVE=1 (the live CI job).',
  );

  interface Detail {
    submission: { id: string; state: string; decision_reason: string | null };
    items: { id: string; ref_id: string; label: string; state: string; note: string | null }[];
  }
  interface Slot {
    upload: { id: string };
    upload_url: string;
    upload_method?: string;
  }

  async function apiGet<T>(request: APIRequestContext, path: string): Promise<T> {
    const res = await request.get(`${API}${path}`);
    if (!res.ok()) throw new Error(`GET ${path} -> ${res.status()}: ${await res.text()}`);
    return (await res.json()) as T;
  }

  async function apiPut<T>(request: APIRequestContext, path: string, data: unknown): Promise<T> {
    const csrf = (await (await request.get(`${API}/sr/csrf`)).json()) as { token: string };
    const res = await request.put(`${API}${path}`, { headers: { 'X-SR-CSRF': csrf.token }, data });
    if (!res.ok()) throw new Error(`PUT ${path} -> ${res.status()}: ${await res.text()}`);
    return (await res.json()) as T;
  }

  /** Send `zip` as the file of a chapter through the routes the page calls, and wait for the conversion. */
  async function sendFile(
    request: APIRequestContext,
    submission: string,
    chapter: string,
    filename: string,
    zip: Buffer,
  ) {
    const base = `/submissions/${submission}/library`;
    const slot = await apiPost<Slot>(request, `${base}/chapters/${chapter}/uploads`, {
      filename,
      size_bytes: zip.length,
    });
    const put = await request.fetch(slot.upload_url, {
      method: slot.upload_method ?? 'PUT',
      data: zip,
    });
    if (!put.ok()) throw new Error(`the presigned PUT answered ${put.status()}`);
    await apiPost(request, `${base}/uploads/${slot.upload.id}/complete`);
    await expect
      .poll(
        async () =>
          (
            await apiGet<{ uploads: { id: string; status: string; error: string | null }[] }>(
              request,
              `${base}/chapters/${chapter}/uploads`,
            )
          ).uploads.find((u) => u.id === slot.upload.id)?.status,
        { timeout: 90_000, message: `chapter ${chapter} conversion` },
      )
      .toBe('done');
  }

  const detailOf = (request: APIRequestContext, id: string) =>
    apiGet<Detail>(request, `/submissions/${id}`);

  /** Page position as the server holds it for this account. */
  const savedPage = async (request: APIRequestContext, chapter: string) => {
    const res = await request.get(`${API}/library/chapters/${chapter}/progress`);
    return res.ok() ? ((await res.json()) as { page_index: number }).page_index : null;
  };

  const status = (page: Page) => page.getByTestId('reader-status');

  test('submit ten chapters, review nine and send one back, read it right to left on a phone', async ({
    seedUser,
    signedIn,
    owner,
  }) => {
    test.setTimeout(10 * 60_000);
    const stamp = Date.now().toString(36);
    const title = `Gate Test ${stamp}`;
    const member = await signedIn(await seedUser('gate'));
    const reviewer = await signedIn(owner);
    const mine = member.context.request;
    const staff = reviewer.context.request;
    const page = member.page;

    // 1. The member starts a new series (manga: right to left) and adds the chapters.
    const created = await test.step('the member starts a series submission', async () => {
      const d = await apiPost<Detail>(mine, '/submissions', {
        kind: 'library.series',
        payload: { kind: 'manga', title, authors: ['E2E'], genres: ['Action'], synopsis: 'Gate.' },
      });
      await page.goto(`/library/submissions/${d.submission.id}`);
      await expect(page.getByRole('heading', { name: title })).toBeVisible();
      return d.submission.id;
    });

    const chapterIds: Record<number, string> = {};
    await test.step('chapter 1 goes through the page: add it, choose the ZIP', async () => {
      await page.getByLabel('Chapter number').fill('1');
      await page.getByRole('button', { name: 'Add chapter' }).click();
      const chooser = page.getByLabel('Choose a file for chapter 1');
      await expect(chooser).toBeVisible();
      await chooser.setInputFiles({
        name: 'chapter-001.zip',
        mimeType: 'application/zip',
        buffer: chapterZip(1),
      });
      // The page checks the archive in the browser, then PUTs it; the conversion reports pages when done.
      await expect(page.getByRole('table', { name: 'Chapters' })).toContainText(`${PAGES} pages`, {
        timeout: 120_000,
      });
    });

    await test.step('chapters 2 to 10 use the same routes', async () => {
      const list = await apiGet<{ chapters: { id: string; number: number }[] }>(
        mine,
        `/submissions/${created}/library/chapters`,
      );
      for (const c of list.chapters) chapterIds[c.number] = c.id;
      for (let n = 2; n <= CHAPTERS; n++) {
        const ch = await apiPost<{ id: string }>(mine, `/submissions/${created}/library/chapters`, {
          number: n,
          title: null,
          format: 'images',
        });
        chapterIds[n] = ch.id;
        await sendFile(
          mine,
          created,
          ch.id,
          `chapter-${String(n).padStart(3, '0')}.zip`,
          chapterZip(n),
        );
      }
      expect(Object.keys(chapterIds)).toHaveLength(CHAPTERS);
    });

    await test.step('the member sends it for review', async () => {
      await page.reload();
      await page.getByRole('button', { name: 'Submit for review' }).click();
      await expect
        .poll(async () => (await detailOf(mine, created)).submission.state)
        .toMatch(/submitted|in_review/);
      expect((await detailOf(mine, created)).items.length).toBeGreaterThanOrEqual(CHAPTERS);
    });

    // 2. The reviewer approves nine chapters, rejects one with a note and asks for changes with a reason.
    const reason = `Chapter ${SENT_BACK}: page 2 is cropped, please send the full page.`;
    const note = 'Page 2 is cropped';
    await test.step('the reviewer approves nine, rejects one and asks for changes', async () => {
      const R = `/admin/review/submissions/${created}`;
      await apiPost(staff, `${R}/claim`, { takeover: false });
      const d = await apiGet<Detail>(staff, R);
      expect(d.items.length).toBeGreaterThanOrEqual(CHAPTERS);
      for (const item of d.items) {
        const sentBack = item.ref_id === chapterIds[SENT_BACK];
        await apiPut(staff, `${R}/items/${item.id}`, {
          state: sentBack ? 'rejected' : 'approved',
          note: sentBack ? note : null,
        });
      }
      await apiPost(staff, `${R}/request-changes`, { reason });
      const after = await apiGet<Detail>(staff, R);
      expect(after.items.filter((i) => i.state === 'approved').length).toBeGreaterThanOrEqual(
        CHAPTERS - 1,
      );
      expect(after.items.filter((i) => i.state === 'rejected')).toHaveLength(1);
      expect(after.submission.state).toBe('changes_requested');
    });

    await test.step('the member sees the reason and the note', async () => {
      await page.reload();
      const banner = page.getByRole('status').filter({ hasText: 'A reviewer asked for changes' });
      await expect(banner).toContainText(reason);
      await expect(page.getByRole('table', { name: 'Chapters' })).toContainText(note);
    });

    // 3. The member replaces the file and sends it again; the reviewer finishes it.
    await test.step('the member fixes chapter 10 and sends it again', async () => {
      await sendFile(
        mine,
        created,
        chapterIds[SENT_BACK]!,
        'chapter-010-v2.zip',
        chapterZip(SENT_BACK, 1),
      );
      await page.reload();
      await page.getByRole('button', { name: 'Send for review again' }).click();
      await expect
        .poll(async () => (await detailOf(mine, created)).submission.state)
        .toMatch(/submitted|in_review/);
    });
    await test.step('the reviewer approves the rest and publishes', async () => {
      const R = `/admin/review/submissions/${created}`;
      await apiPost(staff, `${R}/claim`, { takeover: true });
      const d = await apiGet<Detail>(staff, R);
      for (const item of d.items.filter((i) => i.state !== 'approved'))
        await apiPut(staff, `${R}/items/${item.id}`, { state: 'approved', note: null });
      // Approve publishes too (sr-review `approve`); `publish` is only the retry after a failed publish, and on a
      // published submission it answers 409 (the first CI run).
      const approved = await apiPost<Detail>(staff, `${R}/approve`, { reason: null });
      if (approved.submission.state !== 'published') await apiPost(staff, `${R}/publish`);
      await expect
        .poll(async () => (await detailOf(mine, created)).submission.state, { timeout: 60_000 })
        .toBe('published');
    });

    // 4. Read chapter 1 right to left at a phone size; a second context of the same member follows along.
    const chapter = chapterIds[1]!;
    await page.setViewportSize(PHONE);
    await page.goto(`/library/r/${chapter}`);
    await expect(status(page)).toHaveText(`Page 1 of ${PAGES}`);
    await expect(page.locator('.lib-spread')).toHaveAttribute('data-dir', 'rtl');

    // Right to left: the left arrow is "forward".
    await page.keyboard.press('ArrowLeft');
    await expect(status(page)).toHaveText(`Page 2 of ${PAGES}`);
    await page.keyboard.press('ArrowRight');
    await expect(status(page)).toHaveText(`Page 1 of ${PAGES}`);
    await page.keyboard.press('ArrowLeft');
    await expect(status(page)).toHaveText(`Page 2 of ${PAGES}`);

    const second: SignedIn = await signedIn(member.user);
    await second.page.setViewportSize(PHONE);
    await expect
      .poll(() => savedPage(second.context.request, chapter), { timeout: 20_000 })
      .toBe(1);
    await second.page.goto(`/library/r/${chapter}`);
    await expect(status(second.page)).toHaveText(`Page 2 of ${PAGES}`);

    // The first context turns another page: the account's position moves, and the second context lands on it.
    await page.keyboard.press('ArrowLeft');
    await expect(status(page)).toHaveText(`Page 3 of ${PAGES}`);
    await expect
      .poll(() => savedPage(second.context.request, chapter), { timeout: 20_000 })
      .toBe(2);
    // (A fresh visit, not a reload: the address already carries the page the context was on.)
    await second.page.goto(`/library/r/${chapter}`);
    await expect(status(second.page)).toHaveText(`Page 3 of ${PAGES}`);
  });
});

import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { deflateSync } from 'node:zlib';
import type { APIRequestContext, Browser, BrowserContext, Locator, Page } from '@playwright/test';
import {
  apiPatch,
  bootstrapState,
  expect,
  loginApi,
  registerUser,
  test,
} from '../support/fixtures';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * SEC-AUD-318 (wave 80; D-820, D-829), on the real stack: the Library's reader bars restate the status colours
 * (a probe `var(--ui-danger)` inside the bar computes `rgb(180, 80, 63)` under a light Library), and the pages
 * nobody had looked at: the 18+ badge on a card and on the series page, a chapter's comments and a series' reviews
 * with data from a second account, the PDF reader, the EPUB reader and the reader settings dialog.
 *
 * Data is made through the API as the Owner (Admin routes, no reauthentication on this stack) plus one second
 * account. Presigned PUTs do not work with `storage.endpoint = memory:`, so no file can be uploaded on this stack:
 * the PDF and EPUB chapters are real chapters whose manifest `source_url` is answered by `page.route` with files the
 * spec writes itself (a two-page PDF as a string, an EPUB as a stored zip), and the images chapter's page images are
 * routed to a generated PNG. Shipped (`sectionThemes.library = 'sideroom'`, set explicitly and read back from the
 * room's root) and light (`lounge-light`), desktop and phone.
 * Local only (SR_P80_LIBRARY=1); shots go to SR_P80_LIBRARY_OUT. SR_P80_BASE=1: the base case (probe and figures
 * logged, the probe not asserted).
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P80_LIBRARY_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p80-library';
const BASE = process.env.SR_P80_BASE === '1';
const MODE = BASE ? 'base' : 'after';
const PHONE = { width: 375, height: 812 };
const DESKTOP = { width: 1280, height: 800 };
const API = '/api/v1';

test.skip(process.env.SR_P80_LIBRARY !== '1', 'local visual check only');
test.setTimeout(1_500_000);

type Prefs = { theme: string; zoom: number; sectionThemes?: Record<string, string> };
const prefsFor = (section: string): Prefs => ({
  theme: 'lounge-dark',
  zoom: 100,
  sectionThemes: { library: section },
});

async function context(
  browser: Browser,
  base: BrowserContext,
  size: { width: number; height: number },
  prefs: Prefs,
) {
  const c = await browser.newContext({
    baseURL: ORIGIN,
    viewport: size,
    storageState: await base.storageState(),
    bypassCSP: true,
  });
  await c.addInitScript((p) => {
    localStorage.setItem('sr.user.prefs', JSON.stringify({ state: { prefs: p }, version: 2 }));
  }, prefs);
  return c;
}

const log = (line: string) => console.log(`REPORT ${line}`);
let shots = 0;

// ---------------------------------------------------------------------------------------------------- files

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
const crc32 = (b: Buffer) => {
  let c = 0xffffffff;
  for (const x of b) c = crcTable[(c ^ x) & 0xff]! ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
};

/** A 200x300 PNG of one colour per page. */
function png(n: number): Buffer {
  const w = 200;
  const h = 300;
  const chunk = (type: string, data: Buffer) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
    const sum = Buffer.alloc(4);
    sum.writeUInt32BE(crc32(body));
    return Buffer.concat([len, body, sum]);
  };
  const rows: Buffer[] = [];
  for (let y = 0; y < h; y++) {
    const row = Buffer.alloc(1 + w * 3);
    for (let x = 0; x < w; x++) {
      const o = 1 + x * 3;
      row[o] = (70 + n * 40) % 255;
      row[o + 1] = (110 + y / 3) % 255;
      row[o + 2] = (170 - n * 30 + 255) % 255;
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

/** A two-page PDF written as a string (Helvetica text), with a real cross-reference table. */
function pdf(): Buffer {
  const text = (s: string) =>
    `BT /F1 24 Tf 60 700 Td (${s}) Tj ET BT /F1 12 Tf 60 660 Td (A page of the e2e book for the Library reader.) Tj ET`;
  const objs = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 7 0 R >> >> >>',
    `<< /Length ${text('Page one').length} >>\nstream\n${text('Page one')}\nendstream`,
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R /Resources << /Font << /F1 7 0 R >> >> >>',
    `<< /Length ${text('Page two').length} >>\nstream\n${text('Page two')}\nendstream`,
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
  ];
  let out = '%PDF-1.4\n';
  const offs: number[] = [];
  objs.forEach((o, i) => {
    offs.push(out.length);
    out += `${i + 1} 0 obj\n${o}\nendobj\n`;
  });
  const xref = out.length;
  out += `xref\n0 ${objs.length + 1}\n0000000000 65535 f \n`;
  for (const o of offs) out += `${String(o).padStart(10, '0')} 00000 n \n`;
  out += `trailer\n<< /Size ${objs.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return Buffer.from(out, 'latin1');
}

/** A stored (uncompressed) zip. */
function zipStored(files: [string, Buffer][]): Buffer {
  const locals: Buffer[] = [];
  const central: Buffer[] = [];
  let offset = 0;
  for (const [name, data] of files) {
    const nm = Buffer.from(name, 'utf8');
    const sum = crc32(data);
    const lh = Buffer.alloc(30);
    lh.writeUInt32LE(0x04034b50, 0);
    lh.writeUInt16LE(20, 4);
    lh.writeUInt32LE(sum, 14);
    lh.writeUInt32LE(data.length, 18);
    lh.writeUInt32LE(data.length, 22);
    lh.writeUInt16LE(nm.length, 26);
    const local = Buffer.concat([lh, nm, data]);
    const ch = Buffer.alloc(46);
    ch.writeUInt32LE(0x02014b50, 0);
    ch.writeUInt16LE(20, 4);
    ch.writeUInt16LE(20, 6);
    ch.writeUInt32LE(sum, 16);
    ch.writeUInt32LE(data.length, 20);
    ch.writeUInt32LE(data.length, 24);
    ch.writeUInt16LE(nm.length, 28);
    ch.writeUInt32LE(offset, 42);
    central.push(Buffer.concat([ch, nm]));
    locals.push(local);
    offset += local.length;
  }
  const cd = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(files.length, 8);
  end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(cd.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, cd, end]);
}

/** A small EPUB 3 with three chapters of paragraphs and a table of contents. */
function epub(): Buffer {
  const para = Array.from(
    { length: 14 },
    (_, i) =>
      `<p>Paragraph ${i + 1}. The lamp over the reading table burned low while the long story went on, and the margins held the notes of everyone who had read it before.</p>`,
  ).join('\n');
  const xhtml = (title: string) =>
    `<?xml version="1.0" encoding="utf-8"?>\n<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>${title}</title></head><body><h1>${title}</h1>\n${para}\n</body></html>`;
  const files: [string, Buffer][] = [
    ['mimetype', Buffer.from('application/epub+zip')],
    [
      'META-INF/container.xml',
      Buffer.from(
        '<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>',
      ),
    ],
    [
      'OEBPS/content.opf',
      Buffer.from(
        '<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">p80-epub</dc:identifier><dc:title>The Reading Table</dc:title><dc:language>en</dc:language><meta property="dcterms:modified">2026-11-16T00:00:00Z</meta></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/><item id="c3" href="c3.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/><itemref idref="c2"/><itemref idref="c3"/></spine></package>',
      ),
    ],
    [
      'OEBPS/nav.xhtml',
      Buffer.from(
        '<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head><body><nav epub:type="toc"><ol><li><a href="c1.xhtml">The lamp</a></li><li><a href="c2.xhtml">The margins</a></li><li><a href="c3.xhtml">The end</a></li></ol></nav></body></html>',
      ),
    ],
    ['OEBPS/c1.xhtml', Buffer.from(xhtml('The lamp'))],
    ['OEBPS/c2.xhtml', Buffer.from(xhtml('The margins'))],
    ['OEBPS/c3.xhtml', Buffer.from(xhtml('The end'))],
  ];
  return zipStored(files);
}

// ---------------------------------------------------------------------------------------------------- measuring

/** The sideways-scroll measurement: figures, and the elements whose right edge is past the viewport. */
async function measure(page: Page, name: string) {
  const m = await page.evaluate(() => {
    const de = document.documentElement;
    const vw = de.clientWidth;
    const wide = Array.from(document.querySelectorAll('body *'))
      .filter((e) => {
        const r = e.getBoundingClientRect();
        return r.width > 0 && r.right > vw + 1;
      })
      .slice(0, 8)
      .map(
        (e) =>
          e.tagName +
          '.' +
          String((e as HTMLElement).className).slice(0, 30) +
          '@' +
          Math.round(e.getBoundingClientRect().right),
      );
    return { scrollWidth: de.scrollWidth, clientWidth: vw, wide };
  });
  log(`${name} ${JSON.stringify(m)}`);
  return m;
}

/** Contrast of the text of every element under `root` that has its own text: the worst few and the count below 4.5. */
async function contrast(scope: Locator, name: string) {
  const res = await scope.evaluate((root) => {
    const parse = (s: string) => {
      const m = /rgba?\(([^)]+)\)/.exec(s);
      if (!m) return null;
      const p = m[1]!
        .split(/[ ,/]+/)
        .filter(Boolean)
        .map(Number);
      return { r: p[0]!, g: p[1]!, b: p[2]!, a: p[3] ?? 1 };
    };
    const lum = (c: { r: number; g: number; b: number }) => {
      const f = (v: number) => {
        const x = v / 255;
        return x <= 0.03928 ? x / 12.92 : Math.pow((x + 0.055) / 1.055, 2.4);
      };
      return 0.2126 * f(c.r) + 0.7152 * f(c.g) + 0.0722 * f(c.b);
    };
    const bgOf = (el: Element) => {
      let acc: { r: number; g: number; b: number } = { r: 255, g: 255, b: 255 };
      const chain: { r: number; g: number; b: number; a: number }[] = [];
      for (let e: Element | null = el; e; e = e.parentElement) {
        const c = parse(getComputedStyle(e).backgroundColor);
        if (c && c.a > 0) {
          chain.push(c);
          if (c.a >= 1) break;
        }
      }
      for (const c of chain.reverse())
        acc = {
          r: c.r * c.a + acc.r * (1 - c.a),
          g: c.g * c.a + acc.g * (1 - c.a),
          b: c.b * c.a + acc.b * (1 - c.a),
        };
      return acc;
    };
    const out: { text: string; ratio: number; size: number; tag: string }[] = [];
    const els = [root, ...Array.from(root.querySelectorAll('*'))];
    for (const el of els) {
      const own = Array.from(el.childNodes)
        .filter((n) => n.nodeType === 3)
        .map((n) => n.textContent?.trim() ?? '')
        .join(' ')
        .trim();
      if (!own) continue;
      const cs = getComputedStyle(el);
      if (cs.visibility === 'hidden' || cs.display === 'none') continue;
      const fg = parse(cs.color);
      if (!fg) continue;
      const bg = bgOf(el);
      const a = fg.a;
      const mix = {
        r: fg.r * a + bg.r * (1 - a),
        g: fg.g * a + bg.g * (1 - a),
        b: fg.b * a + bg.b * (1 - a),
      };
      const l1 = lum(mix);
      const l2 = lum(bg);
      const ratio = (Math.max(l1, l2) + 0.05) / (Math.min(l1, l2) + 0.05);
      out.push({
        text: own.slice(0, 40),
        ratio: Math.round(ratio * 100) / 100,
        size: parseFloat(cs.fontSize),
        tag: el.tagName.toLowerCase() + '.' + String((el as HTMLElement).className).slice(0, 24),
      });
    }
    return out;
  });
  const bad = res.filter((r) => r.ratio < 4.5);
  log(
    `contrast ${name} n=${res.length} min=${Math.min(...res.map((r) => r.ratio))} below4.5=${JSON.stringify(bad.slice(0, 8))}`,
  );
  return { all: res, bad };
}

// ---------------------------------------------------------------------------------------------------- the data

type Ids = {
  adultSeries: string;
  adultChapter: string;
  bookSeries: string;
  pdfChapter: string;
  epubChapter: string;
};

async function call<T = any>(
  request: APIRequestContext,
  method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE',
  path: string,
  data?: unknown,
): Promise<T> {
  const headers: Record<string, string> = { Origin: ORIGIN };
  if (method !== 'GET') {
    const csrf = await request.get(`${API}/sr/csrf`, { headers: { Origin: ORIGIN } });
    headers['X-SR-CSRF'] = ((await csrf.json()) as { token: string }).token;
  }
  const res = await request.fetch(`${API}${path}`, {
    method,
    headers,
    ...(data !== undefined ? { data } : {}),
  });
  if (!res.ok()) throw new Error(`${method} ${path} -> ${res.status()}: ${await res.text()}`);
  const text = await res.text();
  return (text ? JSON.parse(text) : undefined) as T;
}

async function makeSeries(
  o: APIRequestContext,
  body: Record<string, unknown>,
  chapters: { number: number; title: string; format: 'images' | 'pdf' | 'epub' }[],
) {
  const series = await call<{ id: string }>(o, 'POST', '/admin/library/series', body);
  const edition = await call<{ id: string }>(
    o,
    'POST',
    `/admin/library/series/${series.id}/editions`,
    { language: 'en', label: 'Official', is_default: true },
  );
  const ids: string[] = [];
  for (const c of chapters) {
    const chapter = await call<{ id: string }>(
      o,
      'POST',
      `/admin/library/editions/${edition.id}/chapters`,
      {
        number: c.number,
        title: c.title,
        format: c.format,
        ...(c.format === 'images' ? {} : { source_key: `library/${series.id}/book.${c.format}` }),
      },
    );
    if (c.format === 'images') {
      await call(
        o,
        'PUT',
        `/admin/library/chapters/${chapter.id}/pages`,
        [0, 1, 2].map((i) => ({
          key: `library/${series.id}/${chapter.id}/00${i + 1}.webp`,
          width: 200,
          height: 300,
          variants: {},
        })),
      );
    }
    await call(o, 'POST', `/admin/library/chapters/${chapter.id}/publish`);
    ids.push(chapter.id);
  }
  await call(o, 'POST', `/admin/library/series/${series.id}/publish`);
  return { series: series.id, chapters: ids };
}

// ---------------------------------------------------------------------------------------------------- the test

test('the Library reader bars and the unseen pages', async ({ browser }) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  await setStudySurfacesWith(octx.request, true);
  const setPrefs = (p: Prefs) =>
    apiPatch(octx.request, '/users/@me/settings', { settings: { user_prefs: p } });

  const second = await registerUser(octx.request, 'rdr80');
  const sctx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(sctx.request, second);

  const stamp = Date.now().toString(36);
  const adult = await makeSeries(
    octx.request,
    {
      kind: 'manga',
      title: `Night Ledger ${stamp}`,
      synopsis: 'A mature series for the 18+ badge. It has a second sentence for the card.',
      genres: ['Drama'],
      authors: ['E2E Author'],
      age_restricted: true,
    },
    [{ number: 1, title: 'The first night', format: 'images' }],
  );
  const book = await makeSeries(
    octx.request,
    {
      kind: 'book',
      title: `The Reading Table ${stamp}`,
      synopsis: 'A book as a PDF and as an EPUB.',
      genres: ['Drama'],
      authors: ['E2E Author'],
    },
    [
      { number: 1, title: 'As a PDF', format: 'pdf' },
      { number: 2, title: 'As an EPUB', format: 'epub' },
    ],
  );
  const ids: Ids = {
    adultSeries: adult.series,
    adultChapter: adult.chapters[0]!,
    bookSeries: book.series,
    pdfChapter: book.chapters[0]!,
    epubChapter: book.chapters[1]!,
  };
  log(`ids ${JSON.stringify(ids)}`);

  // Three comments on the chapter (second account), a long one with an unbroken word; two reviews on the series.
  const bodies = [
    'The linework on page two is the best this artist has done, and the quiet panel at the end made me stop.',
    'Unbroken_word_to_test_the_width_Unbroken_word_to_test_the_width_Unbroken_word_to_test_the_width_end',
    'Short one.',
  ];
  for (const b of bodies)
    await call(sctx.request, 'POST', `/library/chapters/${ids.adultChapter}/comments`, { body: b });
  for (const b of bodies.slice(0, 2))
    await call(sctx.request, 'POST', `/library/series/${ids.adultSeries}/comments`, { body: b });
  await call(sctx.request, 'PUT', `/library/series/${ids.adultSeries}/review`, {
    body: 'A slow burn that pays off. The ending lands because the middle is patient; I would give it more stars if I could. '.repeat(
      2,
    ),
    rating: 5,
  });
  await call(octx.request, 'PUT', `/library/series/${ids.adultSeries}/review`, {
    body: 'Not for everyone, and it says so on the cover.',
    rating: 3,
    spoiler: true,
  });

  const pdfBytes = pdf();
  const epubBytes = epub();
  const png0 = png(0);
  /** Routes of a page: images, source files; the manifest of a pdf/epub chapter has its `source_url` swapped. */
  const wire = async (c: BrowserContext) => {
    await c.route('**/cdn/p80-*.png', (r) =>
      r.fulfill({ status: 200, contentType: 'image/png', body: png0 }),
    );
    await c.route('**/cdn/p80/book.pdf', (r) =>
      r.fulfill({ status: 200, contentType: 'application/pdf', body: pdfBytes }),
    );
    await c.route('**/cdn/p80/book.epub', (r) =>
      r.fulfill({ status: 200, contentType: 'application/epub+zip', body: epubBytes }),
    );
    await c.route(/\/api\/v1\/library\/chapters\/\d+\/pages(\?.*)?$/, async (r) => {
      const res = await r.fetch();
      if (!res.ok()) return r.fulfill({ response: res });
      const body = (await res.json()) as {
        chapter: { format: string };
        pages: { url: string }[];
        source_url?: string | null;
      };
      if (body.chapter.format === 'pdf') body.source_url = `${ORIGIN}/cdn/p80/book.pdf`;
      else if (body.chapter.format === 'epub') body.source_url = `${ORIGIN}/cdn/p80/book.epub`;
      else body.pages.forEach((p, i) => (p.url = `${ORIGIN}/cdn/p80-p${i}.png`));
      return r.fulfill({ response: res, json: body });
    });
  };

  const all: Record<string, unknown> = {};

  try {
    for (const [vname, section] of [
      ['shipped', 'sideroom'],
      ['light', 'lounge-light'],
    ] as const) {
      const prefs = prefsFor(section);
      await setPrefs(prefs);
      for (const [vp, size] of [
        ['phone', PHONE],
        ['desktop', DESKTOP],
      ] as const) {
        const phone = vp === 'phone';
        const c = await context(browser, octx, size, prefs);
        await wire(c);
        const page = await c.newPage();
        page.setDefaultTimeout(30_000);
        const tag = `${vp}-${vname}`;
        const shot = async (what: string) => {
          await page.waitForTimeout(500);
          await page.screenshot({ path: resolve(OUT, `${what}-${tag}.png`) });
          shots++;
        };
        const check = async (what: string) => {
          const m = await measure(page, `${what}-${tag}`);
          all[`${what}-${tag}`] = m;
          expect
            .soft(m.scrollWidth, `${what}-${tag}: no sideways scroll`)
            .toBeLessThanOrEqual(m.clientWidth);
          return m;
        };
        const theme = async (root: string) => {
          const dt = await page.locator(root).first().getAttribute('data-theme');
          log(`data-theme ${root} ${tag} ${dt}`);
          expect(dt).toBe(section);
        };
        const go = async (url: string, ready: string, what: string, root = '.lib-root') => {
          await page.goto(url);
          await expect(page.locator(ready).first()).toBeVisible({ timeout: 60_000 });
          await theme(root);
          await page.waitForTimeout(700);
          return check(what);
        };

        // ---- A card with the 18+ badge: Browse.
        await go('/library/browse', '.lib-content', 'browse');
        const card = page.locator(`a[href*="/library/s/${ids.adultSeries}"]`).first();
        await expect(card).toBeVisible({ timeout: 30_000 });
        const badge = await page
          .locator('.lib-card, [class*="lib-card"]')
          .filter({ hasText: '18+' })
          .first()
          .evaluate((el) => {
            const b = Array.from(el.querySelectorAll('*')).find(
              (e) => e.children.length === 0 && /18\+/.test(e.textContent ?? ''),
            );
            const r = b?.getBoundingClientRect();
            return r ? [r.x, r.y, r.width, r.height].map(Math.round) : null;
          })
          .catch(() => null);
        log(`badge-card-${tag} ${JSON.stringify(badge)}`);
        await card.scrollIntoViewIfNeeded();
        await shot('card-18');
        const cardScope = page
          .locator('.lib-card, [class*="lib-card"]')
          .filter({ hasText: '18+' })
          .first();
        if (await cardScope.count()) await contrast(cardScope, `card-${tag}`);

        // ---- The series page: the badge, the reviews, the comments.
        await go(`/library/s/${ids.adultSeries}`, '.lib-series-head', 'series');
        await shot('series-18');
        const head = page.locator('.lib-series-head').first();
        const headBadge = await contrast(head, `series-head-${tag}`);
        expect.soft(headBadge.bad.filter((b) => /18/.test(b.text))).toEqual([]);

        await page.getByRole('tab', { name: 'Reviews' }).click();
        await expect(page.getByText('A slow burn that pays off').first()).toBeVisible();
        await check('reviews');
        const reviews = page.getByRole('tabpanel').first();
        const rc = await contrast(reviews, `reviews-${tag}`);
        expect.soft(rc.bad, `reviews-${tag}: text under 4.5:1`).toEqual([]);
        await shot('reviews');

        await page.getByRole('tab', { name: 'Comments' }).click();
        await expect(page.getByText('Short one.').first()).toBeVisible({ timeout: 30_000 });
        await check('comments');
        const cm = await contrast(page.getByRole('tabpanel').first(), `comments-${tag}`);
        expect.soft(cm.bad, `comments-${tag}: text under 4.5:1`).toEqual([]);
        await shot('comments');
        if (vname === 'light' && phone) {
          // Click-through: post a comment.
          const box = page.getByLabel('Add a comment').first();
          await box.fill(`Posted from the spec ${stamp}`);
          await page.getByRole('button', { name: 'Post', exact: true }).first().click();
          await expect(page.getByText(`Posted from the spec ${stamp}`).first()).toBeVisible({
            timeout: 30_000,
          });
          log('click: posted a comment, it is listed');
          await shot('comment-posted');
        }

        // ---- The images reader: bars, the probe, the comments panel, the settings dialog.
        await go(`/library/r/${ids.adultChapter}/1`, '.lib-reader', 'reader', '.lib-root');
        await page.waitForTimeout(900);
        const probe = await page.evaluate(() => {
          const out: Record<string, string> = {};
          for (const sel of ['.lib-reader-top', '.lib-reader-bottom']) {
            const bar = document.querySelector(sel);
            if (!bar) continue;
            const s = document.createElement('span');
            s.setAttribute('data-p80-probe', '1');
            s.style.color = 'var(--ui-danger)';
            s.style.background = 'var(--ui-success)';
            s.style.borderColor = 'var(--ui-warning)';
            s.style.outlineColor = 'var(--ui-info)';
            s.style.accentColor = 'var(--ui-danger-text)';
            s.textContent = 'x';
            bar.appendChild(s);
            const cs = getComputedStyle(s);
            out[sel] =
              `${cs.color} | ${cs.backgroundColor} | ${cs.borderTopColor} | ${cs.outlineColor}`;
            s.remove();
          }
          return out;
        });
        log(`probe-${tag} ${JSON.stringify(probe)}`);
        all[`probe-${tag}`] = probe;
        if (!BASE) {
          expect(probe['.lib-reader-top'], 'top bar danger').toMatch(
            /^rgb\(180, 80, 63\) \| rgb\(140, 154, 106\)/,
          );
        }
        expect(await page.locator('[data-p80-probe]').count()).toBe(0);
        await shot('reader-bars');
        await contrast(page.locator('.lib-reader-top'), `reader-top-${tag}`);

        // The comments of the chapter: the reader's panel.
        const commentsBtn = page.getByRole('button', { name: /Comments/ }).first();
        if (await commentsBtn.count()) {
          await commentsBtn.click();
          await expect(page.getByText('Short one.').first()).toBeVisible({ timeout: 30_000 });
          await check('reader-comments');
          const dlg = page.getByRole('dialog').first();
          const cr = await contrast(dlg, `reader-comments-${tag}`);
          expect.soft(cr.bad, `reader-comments-${tag}: text under 4.5:1`).toEqual([]);
          await shot('reader-comments');
          await page.keyboard.press('Escape');
        } else log(`reader-comments-${tag}: no Comments button before the end card`);

        // The settings dialog.
        await page
          .locator('.lib-reader-top')
          .hover()
          .catch(() => {});
        await page.getByRole('button', { name: 'Reader settings' }).first().click();
        const sd = page.getByRole('dialog').first();
        await expect(sd).toBeVisible();
        await page.waitForTimeout(500);
        await check('reader-settings');
        const sdc = await contrast(sd, `reader-settings-${tag}`);
        expect.soft(sdc.bad, `reader-settings-${tag}: labels under 4.5:1`).toEqual([]);
        const dlgBox = await sd.evaluate((el) => {
          const r = el.getBoundingClientRect();
          return { x: r.x, w: r.width, scroll: [el.scrollWidth, el.clientWidth] };
        });
        log(`reader-settings-box-${tag} ${JSON.stringify(dlgBox)}`);
        await shot('reader-settings');
        if (vname === 'light' && phone) {
          const toggles = sd.getByRole('switch');
          const n = await toggles.count();
          if (n) {
            const first = toggles.first();
            const before = await first.getAttribute('aria-checked');
            await first.click();
            const after = await first.getAttribute('aria-checked');
            log(`click: reader setting switch ${before} -> ${after}`);
            expect(after).not.toBe(before);
          } else log('click: no switch in the settings dialog');
        }
        await page.keyboard.press('Escape');

        // ---- The PDF reader.
        await go(`/library/r/${ids.pdfChapter}/1`, 'canvas', 'pdf', '.lib-root');
        await page.waitForTimeout(1200);
        const pdfPage = await page
          .locator('[role=status]')
          .first()
          .textContent()
          .catch(() => '');
        log(`pdf-${tag} status="${pdfPage}"`);
        await shot('pdf-reader');
        await contrast(page.locator('.lib-root').first(), `pdf-${tag}`);
        if (vname === 'light') {
          await page.getByRole('button', { name: 'Next page' }).click();
          await page.waitForTimeout(800);
          const after = await page.locator('[role=status]').first().textContent();
          log(`click: pdf next page: "${pdfPage}" -> "${after}"`);
          expect(after).not.toBe(pdfPage);
          await shot('pdf-reader-page2');
        }

        // ---- The EPUB reader and its settings.
        await go(`/library/r/${ids.epubChapter}/1`, '.lib-epub-host', 'epub', '.lib-root');
        await page.waitForTimeout(2500);
        await shot('epub-reader');
        await contrast(page.locator('.lib-root').first(), `epub-${tag}`);
        if (vname === 'light') {
          const pctBefore = await page
            .getByText(/\d+%/)
            .first()
            .textContent()
            .catch(() => '');
          await page.getByRole('button', { name: 'Next page' }).click();
          await page.waitForTimeout(1200);
          const pctAfter = await page
            .getByText(/\d+%/)
            .first()
            .textContent()
            .catch(() => '');
          log(`click: epub next page: "${pctBefore}" -> "${pctAfter}"`);
          await shot('epub-reader-next');
        }
        await page.getByRole('button', { name: 'Reader settings' }).first().click();
        const ed = page.getByRole('dialog').first();
        await expect(ed).toBeVisible();
        await page.waitForTimeout(500);
        await check('epub-settings');
        const ec = await contrast(ed, `epub-settings-${tag}`);
        expect.soft(ec.bad, `epub-settings-${tag}: labels under 4.5:1`).toEqual([]);
        await shot('epub-settings');
        if (vname === 'light' && phone) {
          const sel = ed.getByLabel('Theme').first();
          try {
            await sel.selectOption({ label: 'Dark' });
            log('click: epub theme set to Dark');
          } catch {
            try {
              await sel.click();
              await page.getByRole('option', { name: 'Dark' }).click();
              log('click: epub theme set to Dark (listbox)');
            } catch (e) {
              log(`click: epub theme not changed: ${String(e).slice(0, 80)}`);
            }
          }
          await page.waitForTimeout(600);
          await shot('epub-settings-changed');
        }
        await c.close();
      }
    }
  } finally {
    writeFileSync(resolve(OUT, `measure-${MODE}.json`), JSON.stringify(all, null, 2));
    log(`SUMMARY shots=${shots}`);
    await setPrefs({ theme: 'lounge-dark', zoom: 100, sectionThemes: { library: 'sideroom' } });
    await setStudySurfacesWith(octx.request, false);
    await sctx.close();
    await octx.close();
  }
});

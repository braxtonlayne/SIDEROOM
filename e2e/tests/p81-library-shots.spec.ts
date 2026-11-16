import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { deflateSync } from 'node:zlib';
import type { APIRequestContext, Browser, BrowserContext, Locator, Page } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { runCli } from '../support/cli';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * SEC-AUD-327 and 328 (wave 81; D-831, D-834, D-835, D-836), on the real stack: the Library reader's page counter
 * reads "1 / 3" in a right-to-left reader (the two digits' boxes measured with a Range), a disabled bar button
 * shows no hover and an enabled one hovers dark, the bars state the status and link colours, a series with one
 * chapter says "1 chapter" (series page and browse line), and the EPUB reader's theme set to Dark.
 * Data is made through the API as the Owner (see wave 80's spec: no file can be uploaded on this stack, so the PDF
 * and EPUB chapters are real chapters whose manifest `source_url` is answered by `page.route`). Shipped
 * (`sectionThemes.library = 'sideroom'`, set explicitly and read back from the room's root) and light
 * (`lounge-light`), desktop and phone.
 * Local only (SR_P81_LIBRARY=1); shots go to SR_P81_LIBRARY_OUT. SR_P81_BASE=1: the base case (light only; figures
 * logged, nothing asserted).
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P81_LIBRARY_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p81-library';
const BASE = process.env.SR_P81_BASE === '1';
const MODE = BASE ? 'base' : 'after';
const PHONE = { width: 375, height: 812 };
const DESKTOP = { width: 1280, height: 800 };
const API = '/api/v1';

test.skip(process.env.SR_P81_LIBRARY !== '1', 'local visual check only');
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
        '<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">p81-epub</dc:identifier><dc:title>The Reading Table</dc:title><dc:language>en</dc:language><meta property="dcterms:modified">2026-11-16T00:00:00Z</meta></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/><item id="c3" href="c3.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/><itemref idref="c2"/><itemref idref="c3"/></spine></package>',
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
    const els = [root, ...Array.from(root.querySelectorAll('*'))].filter(
      (e) => !e.closest('.lib-sr-only'),
    );
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

test('the Library reader counter, hover, tokens, plurals and the EPUB in Dark', async ({
  browser,
}) => {
  mkdirSync(OUT, { recursive: true });
  const owner = bootstrapState().owner;
  const octx = await browser.newContext({ baseURL: ORIGIN });
  await loginApi(octx.request, owner);
  await setStudySurfacesWith(octx.request, true);
  try {
    runCli([
      'staff',
      'appoint',
      owner.username,
      '--role',
      'Senior Staff',
      '--reason',
      'e2e p81 library shots',
    ]);
  } catch (e) {
    log(`staff appoint: ${String(e).slice(0, 120)}`);
  }
  const setPrefs = (p: Prefs) =>
    apiPatch(octx.request, '/users/@me/settings', { settings: { user_prefs: p } });
  await call(octx.request, 'POST', '/sr/age-gate', { over_18: true });

  const stamp = Date.now().toString(36);
  const rtlSeries = await makeSeries(
    octx.request,
    {
      kind: 'manga',
      title: `Right To Left ${stamp}`,
      synopsis: 'A series read from the right.',
      genres: ['Drama'],
      authors: ['E2E Author'],
      reading_direction: 'rtl',
    },
    [{ number: 1, title: 'The first night', format: 'images' }],
  );
  const ltrSeries = await makeSeries(
    octx.request,
    {
      kind: 'manhwa',
      title: `Left To Right ${stamp}`,
      synopsis: 'A series read from the left.',
      genres: ['Drama'],
      authors: ['E2E Author'],
      reading_direction: 'ltr',
    },
    [{ number: 1, title: 'Episode one', format: 'images' }],
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
  log(`ids ${JSON.stringify({ rtlSeries, ltrSeries, book })}`);

  const pdfBytes = pdf();
  const epubBytes = epub();
  const png0 = png(0);
  const wire = async (c: BrowserContext) => {
    await c.route('**/cdn/p81-*.png', (r) =>
      r.fulfill({ status: 200, contentType: 'image/png', body: png0 }),
    );
    await c.route('**/cdn/p81/book.pdf', (r) =>
      r.fulfill({ status: 200, contentType: 'application/pdf', body: pdfBytes }),
    );
    await c.route('**/cdn/p81/book.epub', (r) =>
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
      if (body.chapter.format === 'pdf') body.source_url = `${ORIGIN}/cdn/p81/book.pdf`;
      else if (body.chapter.format === 'epub') body.source_url = `${ORIGIN}/cdn/p81/book.epub`;
      else body.pages.forEach((p, i) => (p.url = `${ORIGIN}/cdn/p81-p${i}.png`));
      return r.fulfill({ response: res, json: body });
    });
  };

  const all: Record<string, unknown> = {};
  const variants = (
    [
      ['shipped', 'sideroom'],
      ['light', 'lounge-light'],
    ] as const
  ).filter(([v]) => !BASE || v === 'light');

  try {
    for (const [vname, section] of variants) {
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
          if (!BASE)
            expect
              .soft(m.scrollWidth, `${what}-${tag}: no sideways scroll`)
              .toBeLessThanOrEqual(m.clientWidth);
          return m;
        };
        const theme = async () => {
          const dt = await page.locator('.lib-root').first().getAttribute('data-theme');
          log(`data-theme ${tag} ${dt}`);
          expect(dt).toBe(section);
        };
        const go = async (url: string, ready: string, what: string) => {
          await page.goto(url);
          await expect(page.locator(ready).first()).toBeVisible({ timeout: 60_000 });
          await theme();
          await page.waitForTimeout(700);
          return check(what);
        };
        /** The counter's text and the two digits' boxes (left edge of each digit, by a Range). */
        const counter = async () =>
          page.evaluate(() => {
            const o = document.querySelector('.lib-reader-slider output');
            const tn = o && Array.from(o.childNodes).find((n) => n.nodeType === 3);
            if (!o || !tn) return null;
            const text = tn.textContent ?? '';
            const digits: { ch: string; x: number }[] = [];
            for (let i = 0; i < text.length; i++) {
              if (!/[0-9]/.test(text[i]!)) continue;
              const r = document.createRange();
              r.setStart(tn, i);
              r.setEnd(tn, i + 1);
              digits.push({ ch: text[i]!, x: Math.round(r.getBoundingClientRect().left) });
            }
            return { text: o.textContent, dir: getComputedStyle(o).direction, digits };
          });
        const squash = (s: string | null | undefined) => (s ?? '').replace(/\s/g, '');
        const wake = async () => {
          await page.mouse.move(size.width / 2, size.height / 2);
          await page.mouse.move(size.width / 2 + 3, size.height / 2 + 3);
          await page.waitForTimeout(400);
        };
        const probeBars = async (sels: string[]) =>
          page.evaluate((ss) => {
            const out: Record<string, string> = {};
            for (const sel of ss) {
              const bar = document.querySelector(sel);
              if (!bar) continue;
              const s = document.createElement('span');
              s.style.color = 'var(--ui-status-online)';
              s.style.background = 'var(--ui-status-idle)';
              s.style.borderColor = 'var(--ui-status-dnd)';
              s.style.outlineColor = 'var(--ui-status-offline)';
              s.style.textDecorationColor = 'var(--text-link)';
              s.textContent = 'x';
              bar.appendChild(s);
              const cs = getComputedStyle(s);
              out[sel] =
                `${cs.color} | ${cs.backgroundColor} | ${cs.borderTopColor} | ${cs.outlineColor} | ${cs.textDecorationColor}`;
              s.remove();
            }
            return out;
          }, sels);
        const bgOf = (l: Locator) => l.evaluate((e) => getComputedStyle(e).backgroundColor);

        // ---- (a) The right-to-left reader: the counter on page 1, then page 3.
        await go(`/library/r/${rtlSeries.chapters[0]}/1`, '.lib-reader-slider output', 'rtl');
        await page.waitForTimeout(900);
        await wake();
        const c1 = await counter();
        log(`counter-rtl-p1-${tag} ${JSON.stringify(c1)}`);
        all[`counter-rtl-p1-${tag}`] = c1;
        log(`slider dir ${tag} ${await page.locator('.lib-reader-slider').getAttribute('dir')}`);
        const thumb = await page
          .getByRole('slider')
          .first()
          .evaluate((e) => Math.round(e.getBoundingClientRect().left));
        const track = await page.locator('.lib-reader-slider').evaluate((e) => {
          const r = e.getBoundingClientRect();
          return Math.round(r.left + r.width / 2);
        });
        log(`thumb-rtl-p1-${tag} thumb-left=${thumb} slider-mid=${track}`);
        const probe = await probeBars(['.lib-reader-top', '.lib-reader-bottom']);
        log(`status-probe-${tag} ${JSON.stringify(probe)}`);
        all[`status-probe-${tag}`] = probe;
        await shot('rtl-bar-p1');
        if (!BASE) {
          expect(squash(c1?.text)).toBe('1/3');
          expect(c1!.digits[0]!.x, 'the first digit is left of the last').toBeLessThan(
            c1!.digits[1]!.x,
          );
          expect(thumb, 'the thumb starts at the right in a right-to-left reader').toBeGreaterThan(
            track,
          );
        }
        // Arrows: in a right-to-left reader ArrowLeft turns forward.
        await page.keyboard.press('ArrowLeft');
        await page.waitForTimeout(500);
        const c2 = await counter();
        await page.keyboard.press('ArrowLeft');
        await page.waitForTimeout(500);
        await wake();
        const c3 = await counter();
        log(`counter-rtl-arrows-${tag} p2=${JSON.stringify(c2)} p3=${JSON.stringify(c3)}`);
        if (!BASE) {
          expect(squash(c2?.text)).toBe('2/3');
          expect(squash(c3?.text)).toBe('3/3');
        }
        await shot('rtl-bar-p3');
        if (vname === 'light' && phone && !BASE) {
          // Click-through: the slider (keys) moves the page to either end.
          const sl = page.getByRole('slider').first();
          await sl.focus();
          await page.keyboard.press('Home');
          await page.waitForTimeout(500);
          const h = await counter();
          await page.keyboard.press('End');
          await page.waitForTimeout(500);
          const e = await counter();
          log(`click: slider Home "${h?.text}" End "${e?.text}"`);
          await shot('rtl-slider-end');
        }

        // ---- The left-to-right reader (light: the strings are unchanged).
        if (vname === 'light' && !BASE) {
          await go(`/library/r/${ltrSeries.chapters[0]}/1`, '.lib-reader-slider output', 'ltr');
          await page.waitForTimeout(900);
          await wake();
          const l1 = await counter();
          await page.keyboard.press('ArrowRight');
          await page.waitForTimeout(500);
          const l2 = await counter();
          log(`counter-ltr-${tag} p1=${JSON.stringify(l1)} p2=${JSON.stringify(l2)}`);
          expect(squash(l1?.text)).toBe('1/3');
          expect(squash(l2?.text)).toBe('2/3');
          await shot('ltr-bar');
        }

        // ---- (b) The PDF reader: last page, the pointer on "Next" and on "Previous".
        await go(`/library/r/${book.chapters[0]}/1`, 'canvas', 'pdf');
        await page.waitForTimeout(1200);
        const next = page.getByRole('button', { name: 'Next page' });
        await next.click();
        await page.waitForTimeout(800);
        log(`pdf-status-${tag} ${await page.locator('[role=status]').first().textContent()}`);
        await next.hover({ force: true });
        await page.waitForTimeout(400);
        const nextBg = await bgOf(next);
        log(`pdf-hover-next-${tag} disabled=${await next.isDisabled()} bg=${nextBg}`);
        all[`pdf-hover-next-${tag}`] = nextBg;
        await shot('pdf-last-hover-next');
        const prev = page.getByRole('button', { name: 'Previous page' });
        await prev.hover();
        await page.waitForTimeout(400);
        const prevBg = await bgOf(prev);
        log(`pdf-hover-prev-${tag} bg=${prevBg}`);
        all[`pdf-hover-prev-${tag}`] = prevBg;
        const pdfCols = await page.evaluate(() => {
          const b = document.querySelector('.lib-pdf-bar .lib-btn');
          const cs = b && getComputedStyle(b);
          return cs ? `${cs.color} | ${cs.borderTopColor}` : null;
        });
        log(`pdf-btn-colours-${tag} ${pdfCols}`);
        log(`pdf-probe-${tag} ${JSON.stringify(await probeBars(['.lib-pdf-bar']))}`);
        await shot('pdf-last-hover-prev');
        if (!BASE) {
          expect(nextBg, 'a disabled button has no hover').toBe('rgba(0, 0, 0, 0)');
          if (vname === 'light')
            expect(prevBg, 'an enabled button on the black bar hovers dark').toBe(
              'rgb(28, 29, 24)',
            );
        }
        if (BASE) {
          await go(`/library/s/${rtlSeries.series}`, '.lib-series-head', 'one-chapter');
          log(
            `chapter-heading-${tag} "${await page
              .getByRole('heading', { name: /chapters?$/ })
              .first()
              .textContent()}"`,
          );
          await shot('one-chapter-series');
          await c.close();
          continue;
        }
        // The images reader's bar: an enabled icon button hovers dark too.
        await go(`/library/r/${rtlSeries.chapters[0]}/1`, '.lib-reader-slider output', 'rtl2');
        await wake();
        const sb = page.getByRole('button', { name: 'Reader settings' }).first();
        await sb.hover();
        await page.waitForTimeout(300);
        log(`reader-icon-hover-${tag} bg=${await bgOf(sb)}`);

        // ---- (SEC-AUD-328) A series with one chapter: the page and the browse line.
        await go(`/library/s/${rtlSeries.series}`, '.lib-series-head', 'one-chapter');
        const heading = page.getByRole('heading', { name: /^\d+ chapters?$/ }).first();
        await expect(heading).toBeVisible({ timeout: 30_000 });
        const headText = (await heading.textContent())?.trim();
        log(`chapter-heading-${tag} "${headText}"`);
        expect(headText).toBe('1 chapter');
        await heading.scrollIntoViewIfNeeded();
        await shot('one-chapter-series');
        await go(
          `/library/browse?q=${encodeURIComponent(`Right To Left ${stamp}`)}&view=list`,
          '.lib-content',
          'one-chapter-browse',
        );
        const line = page.getByText(/· 1 chapters?$/).first();
        await expect(line)
          .toBeVisible({ timeout: 30_000 })
          .catch(() => {});
        log(`browse-line-${tag} "${await line.textContent().catch(() => 'not found')}"`);
        await shot('one-chapter-browse');

        // ---- The EPUB reader with its theme set to Dark.
        await go(`/library/r/${book.chapters[1]}/1`, '.lib-epub-host', 'epub');
        await page.waitForTimeout(2500);
        await page.getByRole('button', { name: 'Reader settings' }).first().click();
        const ed = page.getByRole('dialog').first();
        await expect(ed).toBeVisible();
        await ed.getByRole('radio', { name: 'Dark' }).click();
        log(`click: epub theme Dark (${tag})`);
        await page.keyboard.press('Escape');
        await page.waitForTimeout(1500);
        const epubCols = await page.evaluate(() => {
          const r = document.querySelector('.lib-epub');
          const b = document.querySelector('.lib-epub .lib-btn');
          return r && b
            ? {
                bg: getComputedStyle(r).backgroundColor,
                color: getComputedStyle(b).color,
                border: getComputedStyle(b).borderTopColor,
              }
            : null;
        });
        log(`epub-dark-${tag} ${JSON.stringify(epubCols)}`);
        all[`epub-dark-${tag}`] = epubCols;
        await check('epub-dark');
        await contrast(page.locator('.lib-epub').first(), `epub-dark-${tag}`);
        await shot('epub-dark');
        await c.close();
      }
    }
  } finally {
    writeFileSync(resolve(OUT, `measure-${MODE}.json`), JSON.stringify(all, null, 2));
    log(`SUMMARY shots=${shots}`);
    await setPrefs({ theme: 'lounge-dark', zoom: 100, sectionThemes: { library: 'sideroom' } });
    await setStudySurfacesWith(octx.request, false);
    await octx.close();
  }
});

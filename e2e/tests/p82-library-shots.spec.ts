import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { APIRequestContext, Browser, BrowserContext, Page } from '@playwright/test';
import { apiPatch, bootstrapState, expect, loginApi, test } from '../support/fixtures';
import { runCli } from '../support/cli';
import { setStudySurfacesWith } from '../support/studySwitches';

/**
 * SEC-AUD-339 (wave 82; D-843), on the real stack: the EPUB reader is one ground. For each reader theme (Light,
 * Sepia, Dark), flow (paginated, scrolled), Library theme (shipped `sideroom`, light `lounge-light`) and size
 * (1280x800, 375x812) the shot's pixels are sampled in a line along the top, bottom, left and right of the pane
 * between the bars, and down the middle (the gap between two columns): each must be within 2 per channel of the
 * reader theme's `bg`. The nodes of the pane (the element's shadow tree and each iframe's document) are walked and
 * their computed backgrounds logged (`attachShadow` is forced open by an init script so that the renderer's closed
 * root can be read). The first paragraph's box is logged for the "did not move" check. Data as wave 81's spec: the
 * EPUB chapter's `source_url` is answered by `page.route`.
 * Local only (SR_P82_LIBRARY=1); shots go to SR_P82_LIBRARY_OUT. SR_P82_BASE=1: the base case (figures logged, no
 * assertion; the app files held back by the builder).
 */
const ORIGIN = new URL(process.env.SR_E2E_BASE_URL ?? 'http://localhost:5173').origin;
const OUT =
  process.env.SR_P82_LIBRARY_OUT ??
  'C:/Users/braxt/Desktop/SIDEROOM_wt/visual-sweep-shots/p82-library';
const BASE = process.env.SR_P82_BASE === '1';
const MODE = BASE ? 'base' : 'after';
const PHONE = { width: 375, height: 812 };
const DESKTOP = { width: 1280, height: 800 };
const API = '/api/v1';
const EPUB_KEY = 'sr.library.epub.v1';
const BG: Record<string, [number, number, number]> = {
  light: [255, 255, 255],
  sepia: [0xf4, 0xec, 0xd8],
  dark: [0x16, 0x16, 0x1a],
};

test.skip(process.env.SR_P82_LIBRARY !== '1', 'local visual check only');
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
  // The renderer's shadow root is closed; opened here so that the spec can walk it (read only).
  await c.addInitScript(() => {
    const orig = Element.prototype.attachShadow;
    Element.prototype.attachShadow = function (init: ShadowRootInit) {
      return orig.call(this, { ...init, mode: 'open' });
    };
  });
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
        '<?xml version="1.0" encoding="utf-8"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">p82-epub</dc:identifier><dc:title>The Reading Table</dc:title><dc:language>en</dc:language><meta property="dcterms:modified">2026-11-16T00:00:00Z</meta></metadata><manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/><item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/><item id="c3" href="c3.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c1"/><itemref idref="c2"/><itemref idref="c3"/></spine></package>',
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

// ---------------------------------------------------------------------------------------------------- pixels

/** Sample the shot's pixels along lines of the pane (the host's box) and say how far each is from the theme's bg. */
async function sample(page: Page, rtheme: string, opts: { gap: boolean }) {
  const host = await page.locator('.lib-epub-host').first().boundingBox();
  if (!host) throw new Error('no host');
  const png = await page.screenshot();
  const bg = BG[rtheme]!;
  const res = await page.evaluate(
    async ({ b64, host, bg, gap }) => {
      const blob = await (await fetch(`data:image/png;base64,${b64}`)).blob();
      const bmp = await createImageBitmap(blob);
      const cv = document.createElement('canvas');
      cv.width = bmp.width;
      cv.height = bmp.height;
      const cx = cv.getContext('2d')!;
      cx.drawImage(bmp, 0, 0);
      const x0 = Math.round(host.x);
      const y0 = Math.round(host.y);
      const w = Math.round(host.width);
      const h = Math.round(host.height);
      const dev = (d: Uint8ClampedArray | number[], o: number) =>
        Math.max(
          Math.abs(d[o]! - bg[0]!),
          Math.abs(d[o + 1]! - bg[1]!),
          Math.abs(d[o + 2]! - bg[2]!),
        );
      const line = (name: string, pts: [number, number][]) => {
        let max = 0;
        let off = 0;
        for (const [x, y] of pts) {
          const d = dev(cx.getImageData(x, y, 1, 1).data, 0);
          if (d > max) max = d;
          if (d > 2) off++;
        }
        return { name, max, off, n: pts.length };
      };
      const row = (y: number) =>
        Array.from({ length: Math.floor(w / 3) }, (_, i) => [x0 + i * 3, y] as [number, number]);
      const col = (x: number, a = 0, b = 1) =>
        Array.from(
          { length: Math.floor((h * (b - a)) / 3) },
          (_, i) => [x, y0 + Math.floor(h * a) + i * 3] as [number, number],
        );
      const out = [
        line('top', row(y0 + 2)),
        line('bottom', row(y0 + h - 3)),
        line('left', col(x0 + 2)),
        line('right', col(x0 + w - 3)),
      ];
      if (gap) out.push(line('gap', col(x0 + Math.round(w / 2), 0.02, 0.98)));
      // Everything that is not the bg: its bounding box (the text, so it stays inside the margins).
      let minX = 1e9;
      let minY = 1e9;
      let maxX = -1;
      let maxY = -1;
      let count = 0;
      const img = cx.getImageData(x0, y0, w, h).data;
      for (let y = 0; y < h; y++)
        for (let x = 0; x < w; x++) {
          if (dev(img, (y * w + x) * 4) > 2) {
            count++;
            if (x < minX) minX = x;
            if (x > maxX) maxX = x;
            if (y < minY) minY = y;
            if (y > maxY) maxY = y;
          }
        }
      return { lines: out, offCount: count, inkBox: count ? [minX, minY, maxX, maxY] : null };
    },
    { b64: png.toString('base64'), host, bg, gap: opts.gap },
  );
  return { ...res, host: { x: host.x, y: host.y, w: host.width, h: host.height } };
}

/** Walk the pane: every node from `.lib-epub-host` down through shadow roots and iframes, with its computed bg. */
async function walk(page: Page) {
  return page.evaluate(() => {
    const out: {
      path: string;
      tag: string;
      part: string;
      box: number[];
      bg: string;
      cs: string;
    }[] = [];
    const visit = (el: Element, path: string, depth: number) => {
      const r = el.getBoundingClientRect();
      const cs = getComputedStyle(el);
      const bg = cs.backgroundColor + (cs.backgroundImage !== 'none' ? ' +img' : '');
      const inline = (el as HTMLElement).style?.background ?? '';
      out.push({
        path,
        tag: el.tagName.toLowerCase() + (el.id ? '#' + el.id : ''),
        part: el.getAttribute('part') ?? '',
        box: [Math.round(r.x), Math.round(r.y), Math.round(r.width), Math.round(r.height)],
        bg: bg + (inline ? ` inline[${inline.slice(0, 60)}]` : ''),
        cs: cs.colorScheme,
      });
      if (depth > 9) return;
      const kids: Element[] = Array.from(el.children);
      if (el.shadowRoot) kids.push(...Array.from(el.shadowRoot.children));
      for (const k of kids) visit(k, path + '>' + k.tagName.toLowerCase(), depth + 1);
      if (el instanceof HTMLIFrameElement && el.contentDocument?.documentElement) {
        visit(el.contentDocument.documentElement, path + '>#html', depth + 1);
        if (el.contentDocument.body) visit(el.contentDocument.body, path + '>#body', depth + 1);
      }
    };
    const host = document.querySelector('.lib-epub-host');
    if (host) visit(host, 'host', 0);
    return out.filter((n) => !['style', 'script', 'head'].includes(n.tag));
  });
}

/** The first paragraph's box in page coordinates (through the renderer's open shadow root and the iframe). */
async function firstParagraph(page: Page) {
  return page.evaluate(() => {
    const find = (root: Document | ShadowRoot | Element): HTMLIFrameElement | null => {
      for (const el of Array.from(root.querySelectorAll('*'))) {
        if (el instanceof HTMLIFrameElement) return el;
        if (el.shadowRoot) {
          const f = find(el.shadowRoot);
          if (f) return f;
        }
      }
      return null;
    };
    const host = document.querySelector('.lib-epub-host');
    const f = host ? find(host) : null;
    const p = f?.contentDocument?.querySelector('p');
    if (!f || !p) return null;
    const a = f.getBoundingClientRect();
    const r = p.getBoundingClientRect();
    return [a.x + r.x, a.y + r.y, r.width, r.height].map((n) => Math.round(n * 10) / 10);
  });
}

// ---------------------------------------------------------------------------------------------------- the test

test('the EPUB reader is one ground', async ({ browser }) => {
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
      'e2e p82 library shots',
    ]);
  } catch (e) {
    log(`staff appoint: ${String(e).slice(0, 120)}`);
  }
  const setPrefs = (p: Prefs) =>
    apiPatch(octx.request, '/users/@me/settings', { settings: { user_prefs: p } });
  await call(octx.request, 'POST', '/sr/age-gate', { over_18: true });

  const stamp = Date.now().toString(36);
  const book = await makeSeries(
    octx.request,
    {
      kind: 'book',
      title: `The Reading Table ${stamp}`,
      synopsis: 'A book as an EPUB.',
      genres: ['Drama'],
      authors: ['E2E Author'],
    },
    [{ number: 1, title: 'As an EPUB', format: 'epub' }],
  );
  log(`ids ${JSON.stringify(book)}`);
  const epubBytes = epub();
  const wire = async (c: BrowserContext) => {
    await c.route('**/cdn/p82/book.epub', (r) =>
      r.fulfill({ status: 200, contentType: 'application/epub+zip', body: epubBytes }),
    );
    await c.route(/\/api\/v1\/library\/chapters\/\d+\/pages(\?.*)?$/, async (r) => {
      const res = await r.fetch();
      if (!res.ok()) return r.fulfill({ response: res });
      const body = (await res.json()) as { source_url?: string | null };
      body.source_url = `${ORIGIN}/cdn/p82/book.epub`;
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
  const rthemes = BASE ? (['dark'] as const) : (['dark', 'sepia', 'light'] as const);
  const flows = ['paginated', 'scrolled'] as const;

  const verdict = (key: string, s: Awaited<ReturnType<typeof sample>>, rtheme: string) => {
    for (const l of s.lines) {
      log(`px ${key} ${l.name} max=${l.max} off=${l.off}/${l.n}`);
      if (!BASE && !(l.name === 'bottom' && key.includes('-scrolled-')))
        expect.soft(l.max, `${key} ${l.name} (${rtheme} bg)`).toBeLessThanOrEqual(2);
    }
  };
  const seed = (rtheme: string, flow: string) =>
    [EPUB_KEY, JSON.stringify({ theme: rtheme, flow })] as [string, string];
  const logWalk = async (page: Page, tag: string, all: Record<string, unknown>) => {
    const nodes = await walk(page);
    log(`walk ${tag}`);
    for (const n of nodes)
      log(`  ${n.path} | ${n.tag} part=${n.part} box=${n.box.join(',')} bg=${n.bg} scheme=${n.cs}`);
    all[`walk-${tag}`] = nodes;
  };

  try {
    for (const [vname, section] of variants) {
      const prefs = prefsFor(section);
      await setPrefs(prefs);
      for (const [vp, size] of [
        ['desktop', DESKTOP],
        ['phone', PHONE],
      ] as const) {
        const c = await context(browser, octx, size, prefs);
        await wire(c);
        for (const flow of flows) {
          for (const rtheme of rthemes) {
            const tag = `${rtheme}-${flow}-${vp}-${vname}`;
            const page = await c.newPage();
            page.setDefaultTimeout(30_000);
            await page.addInitScript(([k, v]) => localStorage.setItem(k!, v!), seed(rtheme, flow));
            await page.goto(`/library/r/${book.chapters[0]}/1`);
            await expect(page.locator('.lib-epub-host').first()).toBeVisible({ timeout: 60_000 });
            await page.waitForTimeout(3000);
            const dt = await page.locator('.lib-root').first().getAttribute('data-theme');
            log(`data-theme ${tag} ${dt}`);
            expect(dt).toBe(section);
            const m = await measure(page, tag);
            if (!BASE)
              expect
                .soft(m.scrollWidth, `${tag}: no sideways scroll`)
                .toBeLessThanOrEqual(m.clientWidth);
            const s = await sample(page, rtheme, { gap: vp === 'desktop' && flow === 'paginated' });
            verdict(tag, s, rtheme);
            const fp = await firstParagraph(page);
            log(
              `para ${tag} ${JSON.stringify(fp)} ink=${JSON.stringify(s.inkBox)} host=${JSON.stringify(s.host)}`,
            );
            all[tag] = { lines: s.lines, inkBox: s.inkBox, para: fp, host: s.host, scroll: m };
            const wantShot =
              (BASE && flow === 'paginated' && vp === 'desktop') ||
              (!BASE && flow === 'paginated') ||
              (!BASE && flow === 'scrolled' && rtheme === 'dark' && vname === 'light');
            if (wantShot) {
              await page.screenshot({ path: resolve(OUT, `${MODE}-${tag}.png`) });
              shots++;
            }
            if (rtheme === 'dark' && vname === 'light' && !(BASE && vp === 'phone'))
              await logWalk(page, tag, all);
            // Turn pages: 100 ms after "Next", until the section changes (the book's three sections).
            if (flow === 'paginated' && rtheme === 'dark' && vname === 'light') {
              const next = page.getByRole('button', { name: 'Next' }).first();
              let worst = 0;
              let turns = 0;
              const startLabel = await page.locator('.lib-epub-where').first().textContent();
              for (let i = 0; i < 45; i++) {
                await next.click();
                await page.waitForTimeout(100);
                const t = await sample(page, rtheme, { gap: false });
                turns++;
                for (const l of t.lines) worst = Math.max(worst, l.max);
                const lab = await page.locator('.lib-epub-where').first().textContent();
                if (lab !== startLabel && i > 1) {
                  log(`next-100ms ${tag} section changed after ${turns}: ${startLabel} -> ${lab}`);
                  await page.screenshot({ path: resolve(OUT, `${MODE}-after-next-${tag}.png`) });
                  shots++;
                  break;
                }
              }
              log(`next-100ms ${tag} turns=${turns} worst=${worst}`);
              all[`next-${tag}`] = { turns, worst };
              if (!BASE) expect.soft(worst, `${tag} after Next`).toBeLessThanOrEqual(2);
            }
            await page.close();
          }
        }

        // ---- Click through (light Library): a live theme change, three pages forward and one back; the panels.
        if (vname === 'light') {
          const page = await c.newPage();
          page.setDefaultTimeout(30_000);
          await page.addInitScript(
            ([k, v]) => localStorage.setItem(k!, v!),
            seed('light', 'paginated'),
          );
          await page.goto(`/library/r/${book.chapters[0]}/1`);
          await expect(page.locator('.lib-epub-host').first()).toBeVisible({ timeout: 60_000 });
          await page.waitForTimeout(3000);
          const live = async (rtheme: string, what: string) => {
            await page.waitForTimeout(900);
            const s = await sample(page, rtheme, { gap: vp === 'desktop' });
            verdict(`live-${what}-${vp}`, s, rtheme);
            all[`live-${what}-${vp}`] = s.lines;
          };
          await live('light', 'start-light');
          const dlg = page.getByRole('dialog').first();
          const openSettings = async () => {
            await page.getByRole('button', { name: 'Reader settings' }).first().click();
            await expect(dlg).toBeVisible();
            await page.waitForTimeout(600);
          };
          const closePanel = async () => {
            await page.keyboard.press('Escape');
            await expect(dlg).toBeHidden();
          };
          let n = 0;
          for (const [label, key] of [
            ['Dark', 'dark'],
            ['Sepia', 'sepia'],
            ['Light', 'light'],
            ['Dark', 'dark'],
          ] as const) {
            await openSettings();
            await dlg.getByRole('radio', { name: label }).click();
            log(`click: reader theme ${label} (${vp})`);
            await closePanel();
            await live(key, `to-${key}`);
            if (n === 0) {
              log(
                `root ${await page.evaluate(() => {
                  const r = document.querySelector('.lib-epub') as HTMLElement;
                  return (
                    r.getAttribute('style') +
                    ' | ' +
                    getComputedStyle(r).backgroundColor +
                    ' | ' +
                    getComputedStyle(r).transitionProperty
                  );
                })}`,
              );
              await logWalk(page, `live-${vp}`, all);
            }
            n++;
            await page.screenshot({
              path: resolve(OUT, `${MODE}-live-${n}-${key}-${vp}-${vname}.png`),
            });
            shots++;
          }
          if (vp === 'phone') {
            await openSettings();
            await page.screenshot({
              path: resolve(OUT, `${MODE}-settings-panel-dark-${vp}-${vname}.png`),
            });
            shots++;
            await closePanel();
          }
          await page.waitForTimeout(800);
          if (vp === 'desktop') {
            await page
              .getByRole('button', { name: /contents/i })
              .first()
              .click();
            await page.waitForTimeout(800);
            await page.screenshot({
              path: resolve(OUT, `after-toc-panel-dark-${vp}-${vname}.png`),
            });
            shots++;
            await page.keyboard.press('Escape');
            await page.waitForTimeout(500);
          }
          const label = async () =>
            (await page.locator('.lib-epub-where').first().textContent()) ?? '';
          const seq: string[] = [await label()];
          for (let i = 0; i < 3; i++) {
            await page.keyboard.press('ArrowRight');
            await live('dark', `fwd${i + 1}`);
            seq.push(await label());
          }
          await page.keyboard.press('ArrowLeft');
          await live('dark', 'back1');
          seq.push(await label());
          log(`click-through ${vp}: ${JSON.stringify(seq)}`);
          all[`clickthrough-${vp}`] = seq;
          await page.close();
        }
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

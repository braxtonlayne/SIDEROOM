import { fireEvent, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { http, HttpResponse, server } from '../../test/server';
import { decodeBlurhash, encodeBlurhash, imageMeta, isValidBlurhash } from '../msg/blurhash';
import { at, baseHandlers, calls, me, msg, seedLounge } from './gapsHelpers';

vi.mock('../../store/connect', () => ({
  connectGateway: vi.fn(),
  disconnectGateway: vi.fn(),
  getGateway: vi.fn(() => null),
}));

/** A smooth horizontal red-to-blue gradient. */
function gradient(w: number, h: number): Uint8ClampedArray {
  const px = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++)
    for (let x = 0; x < w; x++) {
      const o = 4 * (x + y * w);
      px[o] = Math.round(255 * (1 - x / (w - 1)));
      px[o + 1] = 40;
      px[o + 2] = Math.round(255 * (x / (w - 1)));
      px[o + 3] = 255;
    }
  return px;
}

describe('blurhash', () => {
  it('knows a well-formed hash from junk', () => {
    expect(isValidBlurhash('LEHV6nWB2yk8pyo0adR*.7kCMdnj')).toBe(true);
    expect(isValidBlurhash('LEHV6nWB2yk8pyo0adR*.7kCMdn')).toBe(false);
    expect(isValidBlurhash('')).toBe(false);
    expect(isValidBlurhash('not a hash at all!!!!')).toBe(false);
    expect(() => decodeBlurhash('nope', 4, 4)).toThrow();
  });

  it('encodes to the length its size byte promises', () => {
    const h = encodeBlurhash(gradient(16, 12), 16, 12, 4, 3);
    expect(h).toHaveLength(4 + 2 * 4 * 3);
    expect(isValidBlurhash(h)).toBe(true);
    expect(encodeBlurhash(gradient(16, 12), 16, 12, 1, 1)).toHaveLength(6);
    expect(() => encodeBlurhash(gradient(4, 4), 4, 4, 0, 3)).toThrow();
    expect(() => encodeBlurhash(new Uint8ClampedArray(3), 4, 4)).toThrow();
  });

  it('decodes a hash back to the picture it came from, roughly', () => {
    const w = 24;
    const h = 16;
    const src = gradient(w, h);
    const error = (cx: number, cy: number) => {
      const out = decodeBlurhash(encodeBlurhash(src, w, h, cx, cy), w, h);
      expect(out).toHaveLength(src.length);
      let sum = 0;
      for (let i = 0; i < src.length; i += 4) {
        for (let c = 0; c < 3; c++) sum += Math.abs(out[i + c]! - src[i + c]!);
        expect(out[i + 3]).toBe(255);
      }
      return { mean: sum / ((src.length / 4) * 3), out };
    };
    const coarse = error(5, 3);
    // Left edge red, right edge blue, as in the source, and close on average.
    expect(coarse.out[0]).toBeGreaterThan(coarse.out[2]!);
    expect(coarse.out[4 * (w - 1) + 2]).toBeGreaterThan(coarse.out[4 * (w - 1)]!);
    expect(coarse.mean).toBeLessThan(25);
    // The colour components add detail: a single average colour is much further off.
    expect(error(1, 1).mean).toBeGreaterThan(coarse.mean * 2);
  });

  it('encodes a flat colour with no AC terms to a plain DC hash', () => {
    const flat = new Uint8ClampedArray(4 * 4 * 4).fill(128);
    const hash = encodeBlurhash(flat, 4, 4, 1, 1);
    const px = decodeBlurhash(hash, 2, 2);
    expect(Math.abs(px[0]! - 128)).toBeLessThan(3);
  });
});

describe('imageMeta', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('reads the size and hashes a downscaled copy', async () => {
    const data = gradient(32, 16);
    vi.stubGlobal(
      'createImageBitmap',
      vi.fn(async () => ({ width: 640, height: 320, close: vi.fn() })),
    );
    vi.stubGlobal(
      'OffscreenCanvas',
      class {
        constructor(
          public width: number,
          public height: number,
        ) {}
        getContext() {
          return { drawImage: vi.fn(), getImageData: () => ({ data }) };
        }
      },
    );
    const meta = await imageMeta(new File(['x'], 'a.png', { type: 'image/png' }));
    expect(meta?.width).toBe(640);
    expect(meta?.height).toBe(320);
    expect(isValidBlurhash(meta!.blurhash!)).toBe(true);
  });

  it('skips non-images, SVG and undecodable files', async () => {
    expect(await imageMeta(new File(['x'], 'a.txt', { type: 'text/plain' }))).toBeNull();
    expect(await imageMeta(new File(['x'], 'a.svg', { type: 'image/svg+xml' }))).toBeNull();
    vi.stubGlobal(
      'createImageBitmap',
      vi.fn(async () => {
        throw new Error('bad');
      }),
    );
    expect(await imageMeta(new File(['x'], 'a.png', { type: 'image/png' }))).toBeNull();
  });
});

describe('placeholders and uploads', () => {
  beforeEach(() => baseHandlers());
  afterEach(() => vi.unstubAllGlobals());

  it('paints a placeholder behind an image until it has loaded', async () => {
    seedLounge({
      messages: [
        msg('100', {
          attachments: [
            {
              id: 'a1',
              filename: 'cat.png',
              size: 10,
              url: '/cdn/attachments/c1/a1/cat.png',
              content_type: 'image/png',
              width: 100,
              height: 80,
              blurhash: 'LEHV6nWB2yk8pyo0adR*.7kCMdnj',
            },
          ],
        }),
      ],
    });
    at('/channels/g1/c1');
    const log = await screen.findByRole('log');
    const img = await within(log).findByRole('img', { name: 'cat.png' });
    expect(log.querySelector('canvas.lng-blurhash')).not.toBeNull();
    fireEvent.load(img);
    await waitFor(() => expect(log.querySelector('canvas.lng-blurhash')).toBeNull());
  });

  it('sends the size and BlurHash of an image with the upload slot request', async () => {
    const data = gradient(32, 24);
    vi.stubGlobal(
      'createImageBitmap',
      vi.fn(async () => ({ width: 800, height: 600, close: vi.fn() })),
    );
    vi.stubGlobal(
      'OffscreenCanvas',
      class {
        constructor(
          public width: number,
          public height: number,
        ) {}
        getContext() {
          return { drawImage: vi.fn(), getImageData: () => ({ data }) };
        }
      },
    );
    seedLounge({ messages: [msg('100', { content: 'hi' })] });
    server.use(
      http.post('*/api/v1/channels/c1/attachments', async ({ request }) => {
        const body = (await request.json()) as { files: { id: string }[] };
        calls.push({ key: 'SLOTS', body });
        return HttpResponse.json({
          attachments: body.files.map((f) => ({
            id: f.id,
            upload_id: 'u1',
            upload_url: '/storage/put/u1',
            upload_filename: 'abc/cat.png',
            upload_url_expires_in: 900,
            max_size: 1_000_000,
          })),
        });
      }),
      http.put('*/storage/put/u1', () => new HttpResponse(null, { status: 200 })),
      http.post('*/api/v1/channels/c1/attachments/u1/complete', () =>
        HttpResponse.json({ id: 'u1', filename: 'cat.png', size: 4, url: '/cdn/x' }),
      ),
      http.post('*/api/v1/channels/c1/messages', () =>
        HttpResponse.json(msg('300', { author: me })),
      ),
    );
    at('/channels/g1/c1');
    const input = await screen.findByTestId('file-input');
    await userEvent.upload(input, new File(['abcd'], 'cat.png', { type: 'image/png' }));
    await screen.findByRole('list', { name: 'Files to upload' });
    // Give the background read a moment to land on the file before sending.
    await new Promise((r) => setTimeout(r, 50));
    await userEvent.click(screen.getByRole('button', { name: 'Send message' }));
    await waitFor(() => expect(calls.some((c) => c.key === 'SLOTS')).toBe(true));
    const body = calls.find((c) => c.key === 'SLOTS')!.body as {
      files: { width: number; height: number; blurhash: string }[];
    };
    expect(body.files[0]).toMatchObject({ width: 800, height: 600 });
    expect(isValidBlurhash(body.files[0]!.blurhash)).toBe(true);
  });
});

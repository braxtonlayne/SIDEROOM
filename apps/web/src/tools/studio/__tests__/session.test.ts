import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createBitmap, type Bitmap } from '../bitmap';
import { DEFAULT_CAPTION } from '../caption';
import { FORMAT_MIME } from '../canvasIo';
import { StudioSession } from '../session';
import { bytesOf, containsBytes, installFakeCanvas, MAGIC, textCalls } from './fakeCanvas';

/** A bitmap with a recognisable 8-byte "secret" colour pattern in a block, and quiet grey elsewhere. */
const SECRET = [201, 17, 93, 255, 210, 30, 100, 255];
function withSecret(): Bitmap {
  const b = createBitmap(40, 40, [128, 128, 128, 255]);
  for (let y = 10; y < 20; y++)
    for (let x = 10; x < 20; x++) b.data.set(SECRET.slice((x % 2) * 4, (x % 2) * 4 + 4), (y * 40 + x) * 4);
  return b;
}
const secretPair = SECRET; // pixel pairs sit side by side in a row

describe('studio session', () => {
  beforeEach(() => installFakeCanvas());
  afterEach(() => vi.restoreAllMocks());

  it('applies crop, resize, pad, rotate and flip in turn, with undo and redo', () => {
    const s = new StudioSession(createBitmap(20, 10, [1, 2, 3, 255]));
    s.crop({ x: 0, y: 0, width: 10, height: 10 });
    s.resize(5, 5);
    s.pad({ top: 1, bottom: 1, left: 2, right: 2 });
    expect([s.current.width, s.current.height]).toEqual([9, 7]);
    s.rotate();
    expect([s.current.width, s.current.height]).toEqual([7, 9]);
    s.flip(true);
    expect(s.canUndo).toBe(true);
    s.undo();
    s.undo();
    expect([s.current.width, s.current.height]).toEqual([9, 7]);
    expect(s.canRedo).toBe(true);
    s.redo();
    expect([s.current.width, s.current.height]).toEqual([7, 9]);
    s.crop({ x: 0, y: 0, width: 2, height: 2 });
    expect(s.canRedo).toBe(false); // a new edit clears the redo stack
  });

  it('burns captions into the pixels', () => {
    const s = new StudioSession(createBitmap(200, 100, [10, 10, 10, 255]));
    s.caption({ ...DEFAULT_CAPTION, top: 'top text', bottom: 'bottom text' });
    expect(textCalls.filter((c) => c.kind === 'fill').map((c) => c.text)).toEqual(['TOP TEXT', 'BOTTOM TEXT']);
    expect([s.current.width, s.current.height]).toEqual([200, 100]);
  });

  it('exports the formats it is asked for at the working size', async () => {
    const s = new StudioSession(createBitmap(6, 4, [9, 8, 7, 255]));
    for (const f of ['png', 'jpeg', 'webp'] as const) {
      const blob = await s.exportAs(f);
      expect(blob.type).toBe(FORMAT_MIME[f]);
      const bytes = await bytesOf(blob);
      expect(Array.from(bytes.slice(0, 4))).toEqual(MAGIC);
      expect([bytes[4], bytes[6]]).toEqual([6, 4]);
    }
  });

  describe('privacy: redaction and pixelation never leave the canvas', () => {
    it('the exported file holds only the redacted pixels', async () => {
      const s = new StudioSession(withSecret());
      expect(containsBytes(await bytesOf(await s.exportAs('png')), secretPair)).toBe(true); // control
      expect(s.redact({ x: 8, y: 8, width: 14, height: 14 })).toBe(true);
      const bytes = await bytesOf(await s.exportAs('png'));
      expect(containsBytes(bytes, secretPair)).toBe(false);
      expect(containsBytes(bytes, SECRET.slice(0, 4))).toBe(false);
      expect(Array.from(s.current.data.subarray((12 * 40 + 12) * 4, (12 * 40 + 12) * 4 + 4))).toEqual([0, 0, 0, 255]);
    });

    it('a pixelated region exports without any of its original pixels', async () => {
      const s = new StudioSession(withSecret());
      s.pixelate({ x: 8, y: 8, width: 14, height: 14 }, 6);
      const bytes = await bytesOf(await s.exportAs('webp'));
      expect(containsBytes(bytes, SECRET.slice(0, 4))).toBe(false);
      expect(containsBytes(bytes, SECRET.slice(4, 8))).toBe(false);
    });

    it('redacting after a crop, resize or caption still removes it from the file', async () => {
      const s = new StudioSession(withSecret());
      s.resize(40, 40);
      s.caption({ ...DEFAULT_CAPTION, top: 'x' });
      s.redact({ x: 0, y: 0, width: 40, height: 40 });
      expect(containsBytes(await bytesOf(await s.exportAs('png')), SECRET.slice(0, 4))).toBe(false);
    });

    it('makes no network request or beacon while editing and exporting', async () => {
      const fetchSpy = vi.fn();
      const xhrSpy = vi.spyOn(XMLHttpRequest.prototype, 'open');
      const beacon = vi.fn();
      vi.stubGlobal('fetch', fetchSpy);
      vi.stubGlobal('WebSocket', vi.fn());
      Object.defineProperty(navigator, 'sendBeacon', { value: beacon, configurable: true });
      const s = new StudioSession(withSecret());
      s.crop({ x: 0, y: 0, width: 30, height: 30 });
      s.redact({ x: 5, y: 5, width: 10, height: 10 });
      s.pixelate({ x: 15, y: 15, width: 10, height: 10 }, 5);
      s.caption({ ...DEFAULT_CAPTION, bottom: 'secret' });
      await s.exportAs('png');
      await s.exportAs('jpeg');
      expect(fetchSpy).not.toHaveBeenCalled();
      expect(xhrSpy).not.toHaveBeenCalled();
      expect(beacon).not.toHaveBeenCalled();
      expect(WebSocket).not.toHaveBeenCalled();
      vi.unstubAllGlobals();
    });

    it('undo can restore a redaction in the session, and forgetHistory makes that impossible', () => {
      const s = new StudioSession(withSecret());
      s.redact({ x: 8, y: 8, width: 14, height: 14 });
      s.forgetHistory();
      s.undo();
      expect(Array.from(s.current.data.subarray((12 * 40 + 12) * 4, (12 * 40 + 12) * 4 + 4))).toEqual([0, 0, 0, 255]);
      expect(s.canUndo).toBe(false);
    });

    it('does not edit the bitmap it was given when it redacts (history keeps the original apart)', () => {
      const original = withSecret();
      const s = new StudioSession(original);
      s.redact({ x: 8, y: 8, width: 14, height: 14 });
      expect(s.current).not.toBe(original);
      expect(Array.from(original.data.subarray((12 * 40 + 12) * 4, (12 * 40 + 12) * 4 + 4))).not.toEqual([0, 0, 0, 255]);
    });
  });
});

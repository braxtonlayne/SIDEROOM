/**
 * Test helper: jsdom has no 2D canvas, so this installs a small fake on HTMLCanvasElement. It keeps a real RGBA
 * backing store (putImageData / getImageData / drawImage between canvases work), records text drawing, and
 * encodes `toBlob` as a tiny container holding the RAW pixels, so a test can scan an "exported file" for bytes.
 */
import { vi } from 'vitest';

export const MAGIC = [0x46, 0x41, 0x4b, 0x45]; // "FAKE"

interface Store {
  w: number;
  h: number;
  data: Uint8ClampedArray;
}
const stores = new WeakMap<HTMLCanvasElement, Store>();
export const textCalls: {
  kind: 'fill' | 'stroke';
  text: string;
  x: number;
  y: number;
  font: string;
}[] = [];

function storeOf(c: HTMLCanvasElement): Store {
  let s = stores.get(c);
  if (!s || s.w !== c.width || s.h !== c.height) {
    s = { w: c.width, h: c.height, data: new Uint8ClampedArray(c.width * c.height * 4) };
    stores.set(c, s);
  }
  return s;
}

export function installFakeCanvas(): void {
  textCalls.length = 0;
  if (typeof globalThis.ImageData === 'undefined') {
    class FakeImageData {
      constructor(
        public data: Uint8ClampedArray,
        public width: number,
        public height: number,
      ) {}
    }
    vi.stubGlobal('ImageData', FakeImageData);
  }
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockImplementation(function (
    this: HTMLCanvasElement,
  ) {
    // eslint-disable-next-line @typescript-eslint/no-this-alias
    const canvas = this;
    return {
      font: '',
      fillStyle: '',
      strokeStyle: '',
      lineWidth: 1,
      lineJoin: 'miter',
      textAlign: 'start',
      textBaseline: 'alphabetic',
      putImageData(img: ImageData) {
        const s = storeOf(canvas);
        s.data.set(img.data.subarray(0, s.data.length));
      },
      getImageData(_x: number, _y: number, w: number, h: number) {
        const s = storeOf(canvas);
        return { width: w, height: h, data: new Uint8ClampedArray(s.data) };
      },
      drawImage(src: unknown) {
        const from = src as HTMLCanvasElement;
        if (stores.has(from)) {
          const to = storeOf(canvas);
          to.data.set(storeOf(from).data.subarray(0, to.data.length));
        }
      },
      measureText(this: { font: string }, text: string) {
        const px = Number(/(\d+)px/.exec(String(this.font))?.[1] ?? 16);
        return { width: text.length * px * 0.6 };
      },
      fillText(this: { font: string }, text: string, x: number, y: number) {
        textCalls.push({ kind: 'fill', text, x, y, font: String(this.font) });
      },
      strokeText(this: { font: string }, text: string, x: number, y: number) {
        textCalls.push({ kind: 'stroke', text, x, y, font: String(this.font) });
      },
      fillRect() {},
    } as unknown as CanvasRenderingContext2D;
  } as never);
  vi.spyOn(HTMLCanvasElement.prototype, 'toBlob').mockImplementation(function (
    this: HTMLCanvasElement,
    cb: BlobCallback,
    type?: string,
  ) {
    const s = storeOf(this);
    const header = new Uint8Array([...MAGIC, s.w & 255, s.w >> 8, s.h & 255, s.h >> 8]);
    cb(new Blob([header, s.data], { type: type ?? 'image/png' }));
  } as never);
}

/** Does `haystack` contain the byte pattern `needle`? */
export function containsBytes(haystack: Uint8Array, needle: number[]): boolean {
  outer: for (let i = 0; i + needle.length <= haystack.length; i++) {
    for (let j = 0; j < needle.length; j++) if (haystack[i + j] !== needle[j]) continue outer;
    return true;
  }
  return false;
}

export function bytesOf(blob: Blob): Promise<Uint8Array> {
  // jsdom's Blob has no arrayBuffer().
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(new Uint8Array(r.result as ArrayBuffer));
    r.onerror = () => reject(r.error);
    r.readAsArrayBuffer(blob);
  });
}

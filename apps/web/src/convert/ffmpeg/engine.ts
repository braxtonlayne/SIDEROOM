// The real engine: ffmpeg.wasm's single-threaded core (no SharedArrayBuffer, so no COOP/COEP headers).
//
// Everything heavy is behind `load()`'s dynamic imports, so none of it is in an entry chunk:
// - `@ffmpeg/ffmpeg` (the class and its worker, a few KB) is its own lazy chunk;
// - `@ffmpeg/core` (the JS glue) and its `.wasm` (about 30 MB) are imported with `?url`, which makes the bundler
//   emit them as separate static files (like JASSUB's) that SIDE ROOM does not touch, so a user can replace them
//   (GPL-2.0-or-later, THIRD_PARTY_NOTICES.md). Nothing is fetched until a person starts a conversion.
import type { FFmpeg } from '@ffmpeg/ffmpeg';
import type { FfmpegEngine } from '../types';

const MOUNT = '/in';

interface Urls {
  coreURL: string;
  wasmURL: string;
}

const importUrls = async (): Promise<Urls> => {
  const [core, wasm] = await Promise.all([
    import('@ffmpeg/core?url'),
    import('@ffmpeg/core/wasm?url'),
  ]);
  return { coreURL: core.default, wasmURL: wasm.default };
};

export class WasmEngine implements FfmpegEngine {
  private ff: FFmpeg | null = null;
  private loading: Promise<FFmpeg> | null = null;
  private mounted = false;
  private sink: ((line: string) => void) | null = null;

  constructor(private urls: () => Promise<Urls> = importUrls) {}

  async load(o?: { signal?: AbortSignal }): Promise<void> {
    this.loading ??= (async () => {
      const [{ FFmpeg: Ctor }, urls] = await Promise.all([import('@ffmpeg/ffmpeg'), this.urls()]);
      const ff = new Ctor();
      ff.on('log', ({ message }) => this.sink?.(message));
      await ff.load(urls, { signal: o?.signal });
      this.ff = ff;
      return ff;
    })().catch((e) => {
      this.loading = null;
      throw e;
    });
    await this.loading;
  }

  private get core(): FFmpeg {
    if (!this.ff) throw new Error('The converter has not loaded yet.');
    return this.ff;
  }

  async mountInput(file: File, name: string): Promise<string> {
    const ff = this.core;
    const input = new File([file], name, { type: file.type });
    try {
      // WORKERFS reads the File in place: the source never has to fit in the wasm heap as well.
      await ff.createDir(MOUNT).catch(() => undefined);
      await ff.mount('WORKERFS' as never, { files: [input] }, MOUNT);
      this.mounted = true;
      return `${MOUNT}/${name}`;
    } catch {
      // A core without WORKERFS: copy the file in (needs twice the memory; the size guard still applies).
      const path = `/${name}`;
      await ff.writeFile(path, new Uint8Array(await input.arrayBuffer()));
      return path;
    }
  }

  async unmountInput(): Promise<void> {
    if (!this.ff || !this.mounted) return;
    this.mounted = false;
    await this.ff.unmount(MOUNT).catch(() => undefined);
  }

  async exec(
    args: string[],
    o?: { signal?: AbortSignal; onLog?: (line: string) => void },
  ): Promise<number> {
    this.sink = o?.onLog ?? null;
    try {
      return await this.core.exec(args, -1, { signal: o?.signal });
    } finally {
      this.sink = null;
    }
  }

  async readFile(path: string): Promise<Uint8Array> {
    const data = await this.core.readFile(path);
    return typeof data === 'string' ? new TextEncoder().encode(data) : data;
  }

  async deleteFile(path: string): Promise<void> {
    await this.core.deleteFile(path);
  }

  terminate(): void {
    this.ff?.terminate();
    this.ff = null;
    this.loading = null;
    this.mounted = false;
  }
}

export const createWasmEngine = (): FfmpegEngine => new WasmEngine();

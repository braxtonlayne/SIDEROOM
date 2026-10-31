// ACT-EMU-001: fetches and starts a core. The cores are EmulatorJS 4.2.x RetroArch builds (emu-ops handoff section
// 1): one `<core>-wasm.data` per core, which is a 7z archive holding `<core>_libretro.js`, `<core>_libretro.wasm`,
// `core.json`, `build.json` and `license.txt`. Unpacking needs the `extract7z.js` worker served beside the cores.
// The glue script defines `EJS_Runtime`, an Emscripten module factory; its RetroArch exports (`simulate_input`,
// `save_state_info`, `load_state`, ...) are what session.ts drives. Nothing here imports EmulatorJS's own loader or
// UI, and nothing is fetched from a CDN: the cores come from this instance's `/emu/cores/` only, after sign-in.
import { coreDataUrl, EXTRACT_7Z_URL, type CoreInfo } from './cores';

/** The slice of an Emscripten module the shell uses. */
export interface CoreModule {
  FS: {
    writeFile(path: string, data: Uint8Array | string): void;
    readFile(path: string): Uint8Array;
    mkdir(path: string): void;
    unlink(path: string): void;
    analyzePath(path: string): { exists: boolean };
  };
  cwrap(name: string, ret: string | null, args: string[]): (...a: unknown[]) => unknown;
  callMain(args: string[]): void;
  resumeMainLoop(): void;
  pauseMainLoop?(): void;
  abort?(): void;
  HEAPU8: Uint8Array;
}

export type ModuleFactory = (opts: Record<string, unknown>) => Promise<CoreModule>;

export class CoreUnavailableError extends Error {
  constructor(
    readonly core: string,
    readonly why: 'missing' | 'format' | 'webgl2' | 'glue' | 'extract',
  ) {
    super(`core ${core}: ${why}`);
  }
}

/** The 7z signature. A dev server that answers a missing file with its index page fails this check. */
export const SEVEN_Z_MAGIC = [0x37, 0x7a, 0xbc, 0xaf, 0x27, 0x1c] as const;
export const isSevenZip = (b: Uint8Array): boolean => SEVEN_Z_MAGIC.every((v, i) => b[i] === v);

export interface CoreFiles {
  js: Uint8Array;
  wasm: Uint8Array;
  license: string;
  /** `core.json` as the archive carries it (name, extensions, save extension, repo). */
  meta: Record<string, unknown>;
}

/** Choose the members of an unpacked archive by suffix. */
export function pickMembers(files: ReadonlyMap<string, Uint8Array>): CoreFiles | null {
  let js: Uint8Array | undefined;
  let wasm: Uint8Array | undefined;
  let license = '';
  let meta: Record<string, unknown> = {};
  const text = (b: Uint8Array) => new TextDecoder().decode(b);
  for (const [name, data] of files) {
    if (name.endsWith('.wasm')) wasm = data;
    else if (name.endsWith('.js') && !name.endsWith('.worker.js')) js = data;
    else if (name.endsWith('license.txt')) license = text(data);
    else if (name.endsWith('core.json')) {
      try {
        meta = JSON.parse(text(data)) as Record<string, unknown>;
      } catch {
        meta = {};
      }
    }
  }
  return js && wasm ? { js, wasm, license, meta } : null;
}

export interface LoaderDeps {
  fetch: typeof fetch;
  /** Unpack a 7z archive. The real one runs the extract7z worker. */
  extract: (
    archive: Uint8Array,
    onProgress?: (done: number, total: number) => void,
  ) => Promise<Map<string, Uint8Array>>;
  /** Run the glue script and return the factory it defines. */
  runGlue: (js: Uint8Array) => Promise<ModuleFactory>;
  supportsWebgl2: () => boolean;
}

/** Unpack with the `extract7z.js` worker: `{t:4}` progress, `{t:2}` one member, `{t:1}` done. */
export function extractWithWorker(
  archive: Uint8Array,
  onProgress?: (done: number, total: number) => void,
  workerUrl: string = EXTRACT_7Z_URL,
): Promise<Map<string, Uint8Array>> {
  return new Promise((resolve, reject) => {
    const files = new Map<string, Uint8Array>();
    let worker: Worker;
    try {
      worker = new Worker(workerUrl);
    } catch {
      reject(new CoreUnavailableError('extract7z', 'extract'));
      return;
    }
    worker.onmessage = (
      ev: MessageEvent<{
        t: number;
        file?: string;
        data?: Uint8Array;
        current?: number;
        total?: number;
      }>,
    ) => {
      const m = ev.data;
      if (m.t === 4) onProgress?.(m.current ?? 0, m.total ?? 0);
      else if (m.t === 2 && m.file && m.data) files.set(m.file, m.data);
      else if (m.t === 1) {
        worker.terminate();
        resolve(files);
      }
    };
    worker.onerror = () => {
      worker.terminate();
      reject(new CoreUnavailableError('extract7z', 'extract'));
    };
    worker.postMessage(archive);
  });
}

/** Run the glue as a blob script and take the `EJS_Runtime` it defines (then remove the global). */
export async function runGlueScript(js: Uint8Array): Promise<ModuleFactory> {
  const w = window as unknown as { EJS_Runtime?: ModuleFactory };
  delete w.EJS_Runtime;
  const url = URL.createObjectURL(new Blob([js as BlobPart], { type: 'text/javascript' }));
  try {
    await new Promise<void>((resolve, reject) => {
      const s = document.createElement('script');
      s.src = url;
      s.onload = () => resolve();
      s.onerror = () => reject(new CoreUnavailableError('glue', 'glue'));
      document.head.appendChild(s);
    });
  } finally {
    URL.revokeObjectURL(url);
  }
  const factory = w.EJS_Runtime;
  delete w.EJS_Runtime;
  if (typeof factory !== 'function') throw new CoreUnavailableError('glue', 'glue');
  return factory;
}

export const realLoaderDeps = (): LoaderDeps => ({
  fetch: (...a) => fetch(...a),
  extract: extractWithWorker,
  runGlue: runGlueScript,
  supportsWebgl2: () => !!document.createElement('canvas').getContext('webgl2'),
});

export interface LoadedCore {
  module: CoreModule;
  files: CoreFiles;
}

export interface LoadOptions {
  canvas: HTMLCanvasElement;
  onProgress?: (stage: 'download' | 'unpack' | 'start', fraction: number) => void;
  onLog?: (line: string) => void;
}

/** Fetch, unpack and instantiate a core. Throws [CoreUnavailableError] with the reason the UI explains. */
export async function loadCore(
  core: CoreInfo,
  opts: LoadOptions,
  deps: LoaderDeps,
): Promise<LoadedCore> {
  if (core.variant === 'webgl2' && !deps.supportsWebgl2()) {
    throw new CoreUnavailableError(core.id, 'webgl2');
  }
  opts.onProgress?.('download', 0);
  let res: Response;
  try {
    res = await deps.fetch(coreDataUrl(core), { credentials: 'same-origin' });
  } catch {
    throw new CoreUnavailableError(core.id, 'missing');
  }
  if (!res.ok) throw new CoreUnavailableError(core.id, 'missing');
  const archive = new Uint8Array(await res.arrayBuffer());
  if (!isSevenZip(archive)) throw new CoreUnavailableError(core.id, 'missing');
  opts.onProgress?.('unpack', 0);
  const members = await deps.extract(archive, (done, total) =>
    opts.onProgress?.('unpack', total > 0 ? done / total : 0),
  );
  const files = pickMembers(members);
  if (!files) throw new CoreUnavailableError(core.id, 'format');
  const factory = await deps.runGlue(files.js);
  opts.onProgress?.('start', 0);
  const wasmUrl = URL.createObjectURL(
    new Blob([files.wasm as BlobPart], { type: 'application/wasm' }),
  );
  const log = (line: string) => opts.onLog?.(line);
  const module = await factory({
    noInitialRun: true,
    canvas: opts.canvas,
    print: log,
    printErr: log,
    locateFile: (name: string) => (name.endsWith('.wasm') ? wasmUrl : name),
    getSavExt: () => '.srm',
  });
  return { module, files };
}

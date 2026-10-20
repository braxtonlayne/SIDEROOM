// JASSUB registration for the ASS seam (CIN-PLY-004, K-13, D-258).
//
// `jassub` (LGPL-2.1-or-later, pinned and unmodified, owner-approved) is imported dynamically, so it is its own
// chunk and never part of a first-load bundle. Its worker and both `.wasm` files are emitted by the bundler as
// separate static files (`?worker&url` / `?url`), not inlined into application code, so a user can replace them
// with a modified build (LGPL-2.1 section 6). If anything fails to load, `startAssRenderer` resolves null and the
// player shows the WebVTT conversion.
import { hasAssRenderer, setAssRendererLoader, type AssRendererLoader } from './ass';

/** The slice of the JASSUB constructor used here, so tests can stand in for the library. */
export interface JassubLike {
  destroy(): Promise<void> | void;
}
export interface JassubModule {
  default: new (opts: {
    video: HTMLVideoElement;
    subUrl: string;
    workerUrl: string;
    wasmUrl: string;
    modernWasmUrl: string;
    queryFonts: 'local';
  }) => JassubLike;
}
export interface JassubAssets {
  workerUrl: string;
  wasmUrl: string;
  modernWasmUrl: string;
}

const importLib = () => import('jassub') as Promise<unknown> as Promise<JassubModule>;
const importAssets = async (): Promise<JassubAssets> => {
  const [w, a, m] = await Promise.all([
    import('jassub/dist/worker/worker.js?worker&url'),
    import('jassub/dist/wasm/jassub-worker.wasm?url'),
    import('jassub/dist/wasm/jassub-worker-modern.wasm?url'),
  ]);
  return { workerUrl: w.default, wasmUrl: a.default, modernWasmUrl: m.default };
};

export function createJassubLoader(
  lib: () => Promise<JassubModule> = importLib,
  assets: () => Promise<JassubAssets> = importAssets,
): AssRendererLoader {
  return async (video, url) => {
    const [{ default: JASSUB }, urls] = await Promise.all([lib(), assets()]);
    const r = new JASSUB({ video, subUrl: url, ...urls, queryFonts: 'local' });
    return { dispose: () => void Promise.resolve(r.destroy()).catch(() => undefined) };
  };
}

/** Registers JASSUB unless a loader is already set (tests set their own). Safe to call repeatedly. */
export function installJassub() {
  if (!hasAssRenderer()) setAssRendererLoader(createJassubLoader());
}

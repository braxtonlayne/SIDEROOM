// ASS / SSA subtitle rendering seam (CIN-PLY-004). Styled subtitles are drawn by JASSUB (a WASM renderer in a
// worker) when a renderer is registered; otherwise the player shows the WebVTT conversion that always exists
// (CIN-ING-008), which loses typesetting but keeps the words.
//
// JASSUB is NOT bundled yet: its npm package is LGPL-2.1-or-later (with GPL/FTL components in the WASM build),
// which is not one of the permissive licences the project has used so far. Adding it is an owner decision; the
// wiring is one call, e.g. in the watch route module:
//
//   setAssRendererLoader(async (video, url) => {
//     const { default: JASSUB } = await import('jassub');
//     const r = new JASSUB({ video, subUrl: url, workerUrl, wasmUrl });
//     return { dispose: () => void r.destroy() };
//   });
//
// Everything else (the menu, fallback on failure, disposal on track change) already works through this seam.

export interface AssRenderer {
  dispose(): void;
}
export type AssRendererLoader = (video: HTMLVideoElement, assUrl: string) => Promise<AssRenderer>;

let loader: AssRendererLoader | null = null;

export function setAssRendererLoader(l: AssRendererLoader | null) {
  loader = l;
}
export const hasAssRenderer = () => loader !== null;

/** Starts the renderer, or resolves null when none is registered or it fails to load (the caller uses VTT). */
export async function startAssRenderer(
  video: HTMLVideoElement,
  assUrl: string,
): Promise<AssRenderer | null> {
  if (!loader) return null;
  try {
    return await loader(video, assUrl);
  } catch {
    return null;
  }
}

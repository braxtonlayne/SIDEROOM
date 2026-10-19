// ASS / SSA subtitle rendering seam (CIN-PLY-004). Styled subtitles are drawn by JASSUB (a WASM renderer in a
// worker) when a renderer is registered; otherwise the player shows the WebVTT conversion that always exists
// (CIN-ING-008), which loses typesetting but keeps the words.
//
// JASSUB is registered by `jassubLoader.ts` (K-13, M-08, D-297, D-309): lazy chunk, worker and .wasm as separate files.
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

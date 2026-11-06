import { usePrefsStore } from './prefs';

/**
 * D-540: the member's zoom setting for the route-outliving overlays (the utility drawer, the gadgets, the pin rail,
 * the player dock and the modals they open). The overlay root is `display: contents`, so it cannot carry `zoom`
 * itself; it carries this custom property instead, and `overlay-root.css` applies it to each positioned child.
 */
export const OVERLAY_ZOOM_VAR = '--sr-overlay-zoom';

/** The zoom factor for a saved zoom percent (100 -> 1). Anything unusable reads as 1. */
export function zoomFactor(percent: unknown): number {
  return typeof percent === 'number' && Number.isFinite(percent) && percent > 0 ? percent / 100 : 1;
}

/** The overlays' zoom factor right now (from the saved prefs, as the overlay root renders it). */
export function overlayZoom(): number {
  return zoomFactor(usePrefsStore.getState().prefs.zoom);
}

/** Subscribes to the saved zoom percent (re-renders only when it changes). */
export const useOverlayZoom = (): number => zoomFactor(usePrefsStore((s) => s.prefs.zoom));

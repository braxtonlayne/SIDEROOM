import type { ToolsApi } from '@sideroom/wasm';

let loaded: Promise<ToolsApi> | undefined;

/** The sr-tools-wasm module (palette extraction, GIF encoding), loaded once on first use. */
export function loadStudioWasm(): Promise<ToolsApi> {
  loaded ??= import('@sideroom/wasm').then((m) => m.loadTools());
  return loaded;
}

import { afterEach, describe, expect, it, vi } from 'vitest';
import { hasAssRenderer, setAssRendererLoader, startAssRenderer } from '../ass';
import { createJassubLoader, installJassub, type JassubModule } from '../jassubLoader';

const assets = () =>
  Promise.resolve({ workerUrl: '/w.js', wasmUrl: '/a.wasm', modernWasmUrl: '/m.wasm' });

afterEach(() => setAssRendererLoader(null));

describe('createJassubLoader (library stubbed)', () => {
  it('starts the renderer with separate worker and wasm urls and disposes it', async () => {
    const destroy = vi.fn(() => Promise.resolve());
    const ctor = vi.fn(function () {
      return { destroy };
    });
    const lib = () => Promise.resolve({ default: ctor } as unknown as JassubModule);
    const video = document.createElement('video');
    const r = await createJassubLoader(lib, assets)(video, '/s.ass');
    expect(ctor).toHaveBeenCalledWith(
      expect.objectContaining({
        video,
        subUrl: '/s.ass',
        workerUrl: '/w.js',
        wasmUrl: '/a.wasm',
        modernWasmUrl: '/m.wasm',
      }),
    );
    r.dispose();
    expect(destroy).toHaveBeenCalled();
  });

  it('falls back to WebVTT (null) when the library fails to load', async () => {
    setAssRendererLoader(createJassubLoader(() => Promise.reject(new Error('chunk')), assets));
    expect(await startAssRenderer(document.createElement('video'), '/s.ass')).toBeNull();
  });

  it('falls back when the renderer throws (no WebGL)', async () => {
    const lib = () =>
      Promise.resolve({
        default: vi.fn(function () {
          throw new Error('no webgl');
        }),
      } as unknown as JassubModule);
    setAssRendererLoader(createJassubLoader(lib, assets));
    expect(await startAssRenderer(document.createElement('video'), '/s.ass')).toBeNull();
  });

  it('installJassub registers once and keeps an existing loader', () => {
    expect(hasAssRenderer()).toBe(false);
    installJassub();
    expect(hasAssRenderer()).toBe(true);
    const own = () => Promise.resolve({ dispose: () => undefined });
    setAssRendererLoader(own);
    installJassub();
    expect(hasAssRenderer()).toBe(true);
  });
});

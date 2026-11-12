import { describe, expect, it, vi } from 'vitest';
import { canCapture, captureTab } from '../capture';

const fakeStream = () => {
  const stops = [vi.fn(), vi.fn()];
  const stream = { getTracks: () => stops.map((stop) => ({ stop })) } as unknown as MediaStream;
  return { stream, stops };
};

describe('captureTab', () => {
  it('asks for the current tab and stops every track after a good grab', async () => {
    const { stream, stops } = fakeStream();
    const getDisplayMedia = vi.fn().mockResolvedValue(stream);
    const r = await captureTab({ getDisplayMedia }, async () => 'frame');
    expect(r).toEqual({ ok: true, frame: 'frame' });
    expect(getDisplayMedia).toHaveBeenCalledWith({
      video: { displaySurface: 'browser' },
      preferCurrentTab: true,
      selfBrowserSurface: 'include',
      audio: false,
    });
    stops.forEach((s) => expect(s).toHaveBeenCalledTimes(1));
  });

  it('calls getDisplayMedia synchronously (the key press is still the gesture)', () => {
    const getDisplayMedia = vi.fn().mockResolvedValue(fakeStream().stream);
    void captureTab({ getDisplayMedia }, async () => 1);
    expect(getDisplayMedia).toHaveBeenCalledTimes(1);
  });

  it('stops the tracks and says failed when the grab throws', async () => {
    const { stream, stops } = fakeStream();
    const r = await captureTab({ getDisplayMedia: () => Promise.resolve(stream) }, async () => {
      throw new Error('boom');
    });
    expect(r).toEqual({ ok: false, reason: 'failed' });
    stops.forEach((s) => expect(s).toHaveBeenCalledTimes(1));
  });

  it('maps the rejections', async () => {
    const rej = (name: string) => ({
      getDisplayMedia: () => Promise.reject(Object.assign(new Error('x'), { name })),
    });
    const grab = async () => 1;
    expect(await captureTab(rej('NotAllowedError'), grab)).toEqual({
      ok: false,
      reason: 'declined',
    });
    expect(await captureTab(rej('AbortError'), grab)).toEqual({ ok: false, reason: 'declined' });
    expect(await captureTab(rej('NotReadableError'), grab)).toEqual({
      ok: false,
      reason: 'failed',
    });
    expect(await captureTab({}, grab)).toEqual({ ok: false, reason: 'unsupported' });
    expect(await captureTab(undefined, grab)).toEqual({ ok: false, reason: 'unsupported' });
  });

  it('canCapture', () => {
    expect(canCapture({ getDisplayMedia: vi.fn() })).toBe(true);
    expect(canCapture({})).toBe(false);
    expect(canCapture(null)).toBe(false);
  });
});

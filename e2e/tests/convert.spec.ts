// BYOS-CNV-001: remux and subtitle extraction with the real ffmpeg.wasm core, in a real browser.
// The source is a generated 10 s MKV (ffmpeg from apt, as media.yml does): H.264 + AAC + one SubRip track.
// The spec drives the converter module through the dev server (`/src/convert/index.ts`), so it needs no sign-in.
// Not run locally in wave 29 (ports 8080/5173 belong to one agent and the machine has no ffmpeg): CI runs it.
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { expect, test } from '@playwright/test';

function ffmpegAvailable(): boolean {
  try {
    execFileSync('ffmpeg', ['-version'], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

function makeMkv(): Buffer {
  const dir = mkdtempSync(join(tmpdir(), 'sr-convert-'));
  const srt = join(dir, 'in.srt');
  writeFileSync(srt, '1\n00:00:01,000 --> 00:00:03,000\nHello from the subtitle\n\n');
  const out = join(dir, 'in.mkv');
  execFileSync(
    'ffmpeg',
    [
      '-y',
      '-f',
      'lavfi',
      '-i',
      'testsrc=duration=10:size=320x240:rate=25',
      '-f',
      'lavfi',
      '-i',
      'sine=frequency=440:duration=10',
      '-i',
      srt,
      '-c:v',
      'libx264',
      '-pix_fmt',
      'yuv420p',
      '-c:a',
      'aac',
      '-c:s',
      'srt',
      '-metadata:s:s:0',
      'language=eng',
      // No -shortest: the subtitle stream is a stream too, and its one cue ends at 3 s, so -shortest cut the whole
      // file to 3 s (CI run 41222719038, duration 3.018). Both lavfi sources end at 10 s on their own.
      out,
    ],
    { stdio: 'ignore' },
  );
  return readFileSync(out);
}

test.describe('convert (ffmpeg.wasm remux)', () => {
  test.skip(!ffmpegAvailable(), 'ffmpeg is not installed');
  test.setTimeout(240_000);

  test('remuxes a 10 s MKV to an MP4 that plays, and extracts the subtitle', async ({ page }) => {
    const b64 = makeMkv().toString('base64');
    await page.goto('/');
    const result = await page.evaluate(async (data) => {
      const bytes = Uint8Array.from(atob(data), (c) => c.charCodeAt(0));
      const file = new File([bytes], 'Film.mkv', { type: 'video/x-matroska' });
      // A Vite dev-server path, not one tsc can resolve from e2e/: keep it a string so the module is untyped here.
      const convertModule: string = '/src/convert/index.ts';
      const mod = await import(/* @vite-ignore */ convertModule);
      const conv = mod.getConverter();
      const probe = await conv.probe(file);
      const r = await conv.remux(file, { probe });
      const s = await conv.extractSubtitles(file, { probe });
      const video = document.createElement('video');
      video.muted = true;
      video.src = URL.createObjectURL(r.output.file);
      await new Promise<void>((res, rej) => {
        video.onloadedmetadata = () => res();
        video.onerror = () => rej(new Error('the remuxed file does not load'));
      });
      return {
        container: probe.container,
        name: r.output.name,
        duration: video.duration,
        subtitles: await Promise.all(
          s.subtitles.map(async (x: { name: string; file: File }) => ({
            name: x.name,
            text: await x.file.text(),
          })),
        ),
      };
    }, b64);
    expect(result.container).toContain('matroska');
    expect(result.name).toBe('Film.mp4');
    expect(result.duration).toBeGreaterThan(9);
    expect(result.duration).toBeLessThan(11);
    expect(result.subtitles).toHaveLength(1);
    expect(result.subtitles[0]?.name).toBe('Film.eng.srt');
    expect(result.subtitles[0]?.text).toContain('Hello from the subtitle');
  });
});

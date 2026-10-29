import { describe, expect, it } from 'vitest';
import { decideRemux } from '../index';
import { parseProbeLog } from '../probe';
import { planWithWebCodecs, sourceContainer, sourceInfo } from '../webcodecsBridge';
import type { Capabilities } from '../webcodecs/types';

const probeOf = (video: string, audio: string, demuxer = 'matroska,webm') =>
  parseProbeLog([
    `Input #0, ${demuxer}, from 'x':`,
    '  Duration: 00:00:10.00, start: 0.0, bitrate: 900 kb/s',
    `  Stream #0:0: Video: ${video}, 1920x1080, 24 fps`,
    `  Stream #0:1(eng): Audio: ${audio}, 48000 Hz, stereo`,
  ]);

/** A browser shape: what it plays, and whether WebCodecs decodes and encodes. */
function caps(plays: RegExp, webcodecs: boolean): Capabilities {
  return {
    engine: 'chromium',
    canPlay: (m) => plays.test(m),
    videoDecode: async () => webcodecs,
    audioDecode: async () => webcodecs,
    videoEncode: async () => webcodecs,
    audioEncode: async () => webcodecs,
  };
}

describe('webcodecsBridge (the real WebCodecs module, integrated)', () => {
  it("maps ffmpeg's demuxer names to the table's containers", () => {
    expect(sourceContainer('matroska,webm', 'Film.mkv')).toBe('mkv');
    expect(sourceContainer('matroska,webm', 'Clip.WebM')).toBe('webm');
    expect(sourceContainer('mov,mp4,m4a,3gp,3g2,mj2', 'a.mov')).toBe('mp4');
    expect(sourceContainer('mpegts', 'a.ts')).toBe('ts');
    expect(sourceContainer('avi', 'a.avi')).toBe('avi');
    expect(sourceContainer('flv', 'a.flv')).toBe('unknown');
  });

  it('builds the WebCodecs codec strings from the probe', () => {
    const info = sourceInfo(probeOf('hevc (Main)', 'ac3'), new File(['x'], 'Film.mkv'));
    expect(info.container).toBe('mkv');
    expect(info.video?.codec).toMatch(/^hvc1\./);
    expect(info.audio?.codec).toBe('ac-3');
    expect(info.durationSec).toBe(10);
  });

  it('H.264 + AAC in MKV is a remux; the remux rule agrees', async () => {
    const probe = probeOf('h264 (High)', 'aac (LC)');
    const file = new File(['x'], 'Film.mkv');
    const plan = await planWithWebCodecs(probe, file, caps(/avc1|mp4a/, false));
    expect(plan?.path).toBe('remux');
    expect(decideRemux(probe).route).toBe('remux');
  });

  it('HEVC where the browser has no HEVC goes to WebCodecs when it can, else to the worker', async () => {
    const probe = probeOf('hevc (Main)', 'aac (LC)');
    const file = new File(['x'], 'Film.mkv');
    const yes = await planWithWebCodecs(probe, file, caps(/avc1|mp4a/, true));
    expect(yes?.path).toBe('webcodecs');
    const no = await planWithWebCodecs(probe, file, caps(/avc1|mp4a/, false));
    expect(no?.path).toBe('worker');
  });
});

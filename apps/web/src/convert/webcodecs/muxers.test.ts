import { beforeAll, describe, expect, it, vi } from 'vitest';
import { createMuxer } from './muxers';

// The muxers check chunks with instanceof, so jsdom gets stand-in classes.
class FakeChunk {}
beforeAll(() => {
  vi.stubGlobal('EncodedVideoChunk', FakeChunk);
  vi.stubGlobal('EncodedAudioChunk', FakeChunk);
});

// The real mp4-muxer and webm-muxer, fed fake chunks (jsdom has no WebCodecs).
function fakeVideoChunk(ts: number, key: boolean) {
  const data = new Uint8Array([0, 0, 0, 2, 0x65, 0x88]);
  return Object.assign(new FakeChunk(), {
    type: key ? 'key' : 'delta',
    timestamp: ts,
    duration: 33333,
    byteLength: data.length,
    copyTo: (dst: Uint8Array) => dst.set(data),
  }) as unknown as EncodedVideoChunk;
}

function fakeAudioChunk(ts: number) {
  const data = new Uint8Array([1, 2, 3, 4]);
  return Object.assign(new FakeChunk(), {
    type: 'key',
    timestamp: ts,
    duration: 21333,
    byteLength: data.length,
    copyTo: (dst: Uint8Array) => dst.set(data),
  }) as unknown as EncodedAudioChunk;
}

const blobBytes = (b: Blob) =>
  new Promise<Uint8Array>((res, rej) => {
    const r = new FileReader();
    r.onload = () => res(new Uint8Array(r.result as ArrayBuffer));
    r.onerror = () => rej(r.error);
    r.readAsArrayBuffer(b);
  });

const ascii = (b: Uint8Array, o: number, n: number) => String.fromCharCode(...b.subarray(o, o + n));

describe('muxers', () => {
  it('writes an MP4 with H.264 and AAC into memory', async () => {
    const mux = await createMuxer({
      container: 'mp4',
      video: { codec: 'avc', width: 320, height: 240, frameRate: 30 },
      audio: { codec: 'aac', sampleRate: 48000, channels: 2 },
      output: { kind: 'memory' },
    });
    const avcC = new Uint8Array([1, 0x64, 0, 0x1f, 0xff, 0xe1, 0, 0]);
    mux.addVideo(fakeVideoChunk(0, true), {
      decoderConfig: { codec: 'avc1.64001f', codedWidth: 320, codedHeight: 240, description: avcC },
    });
    mux.addVideo(fakeVideoChunk(33333, false));
    mux.addAudio(fakeAudioChunk(0), {
      decoderConfig: {
        codec: 'mp4a.40.2',
        sampleRate: 48000,
        numberOfChannels: 2,
        description: new Uint8Array([0x11, 0x90]),
      },
    });
    const blob = mux.finalize()!;
    expect(blob.type).toBe('video/mp4');
    const bytes = await blobBytes(blob);
    expect(ascii(bytes, 4, 4)).toBe('ftyp');
    expect(bytes.length).toBeGreaterThan(200);
  });

  it('writes a WebM with VP9 and Opus into memory', async () => {
    const mux = await createMuxer({
      container: 'webm',
      video: { codec: 'vp9', width: 320, height: 240 },
      audio: { codec: 'opus', sampleRate: 48000, channels: 2 },
      output: { kind: 'memory' },
    });
    mux.addVideo(fakeVideoChunk(0, true));
    mux.addAudio(fakeAudioChunk(0));
    const blob = mux.finalize()!;
    expect(blob.type).toBe('video/webm');
    const bytes = await blobBytes(blob);
    expect([...bytes.subarray(0, 4)]).toEqual([0x1a, 0x45, 0xdf, 0xa3]); // EBML magic
  });

  it('streams through onData and returns no blob', async () => {
    const parts: number[] = [];
    const mux = await createMuxer({
      container: 'webm',
      video: { codec: 'vp9', width: 320, height: 240 },
      output: { kind: 'stream', onData: (d) => parts.push(d.length) },
    });
    mux.addVideo(fakeVideoChunk(0, true));
    expect(mux.finalize()).toBeNull();
    expect(parts.length).toBeGreaterThan(0);
  });

  it('refuses codecs the container cannot hold', async () => {
    await expect(
      createMuxer({
        container: 'webm',
        video: { codec: 'avc', width: 2, height: 2 },
        output: { kind: 'memory' },
      }),
    ).rejects.toMatchObject({ code: 'mux' });
    await expect(
      createMuxer({
        container: 'mp4',
        video: { codec: 'vp8', width: 2, height: 2 },
        output: { kind: 'memory' },
      }),
    ).rejects.toMatchObject({ code: 'mux' });
  });
});

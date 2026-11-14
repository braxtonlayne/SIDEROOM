import { expect, test } from '@playwright/test';
import { musicVerdict, type InboundAudioStream } from '../support/voice';

/**
 * The music check of the voice soak (SEC-AUD-251, D-724) as a function, tried on cases that pass and fail. It
 * needs no page and no stack: the `e2e` jobs run it with the other specs (the global setup still runs first).
 */
const stream = (id: string, over: Partial<InboundAudioStream> = {}): InboundAudioStream => ({
  id,
  bytes: 1000,
  mimeType: 'audio/opus',
  channels: 2,
  ...over,
});
// The microphones that were there before the music: mono as far as the check cares, ids the music never takes.
const before = [stream('mic-a', { channels: 1 })];

test.describe('musicVerdict', () => {
  test('no new stream: the music never arrived', () => {
    const after = [stream('mic-a', { channels: 1, bytes: 5000 })];
    expect(musicVerdict(before, after, [])).toBe('no new audio stream');
  });

  test('a new mono stream is not the music', () => {
    const after = [...before, stream('new-1', { channels: 1 })];
    expect(musicVerdict(before, after, [{ id: 'new-1', bytesBefore: 0, bytesAfter: 900 }])).toBe(
      'the new stream is mono',
    );
  });

  test('a new stereo stream that is not Opus is named', () => {
    const after = [...before, stream('new-1', { mimeType: 'audio/PCMU' })];
    expect(musicVerdict(before, after, [{ id: 'new-1', bytesBefore: 0, bytesAfter: 900 }])).toBe(
      'the new stream is not Opus (audio/PCMU)',
    );
  });

  test('a new stereo stream with no growth in the closing window stopped', () => {
    const after = [...before, stream('new-1')];
    expect(
      musicVerdict(before, after, [{ id: 'new-1', bytesBefore: 1000, bytesAfter: 1000 }]),
    ).toBe('the stereo stream stopped: 0 bytes in the closing window');
  });

  test('a new stereo stream that grows is good', () => {
    const after = [...before, stream('new-1', { bytes: 90_000 })];
    expect(
      musicVerdict(before, after, [{ id: 'new-1', bytesBefore: 40_000, bytesAfter: 90_000 }]),
    ).toBeNull();
  });

  test('a stream missing from the closing window counts from zero', () => {
    const after = [...before, stream('new-1', { bytes: 3000 })];
    expect(musicVerdict(before, after, [])).toBeNull();
    expect(musicVerdict(before, [...before, stream('new-2', { bytes: 0 })], [])).toBe(
      'the stereo stream stopped: 0 bytes in the closing window',
    );
  });

  test('two new streams, one of them the music, is good', () => {
    const after = [
      ...before,
      stream('new-mic', { channels: 1, bytes: 8000 }),
      stream('new-music', { bytes: 70_000 }),
    ];
    const tail = [
      { id: 'new-mic', bytesBefore: 4000, bytesAfter: 8000 },
      { id: 'new-music', bytesBefore: 30_000, bytesAfter: 70_000 },
    ];
    expect(musicVerdict(before, after, tail)).toBeNull();
  });

  test('the old microphone streams are never taken for the music', () => {
    // A stereo stream that was already there before the music started is not new, however much it grows.
    const old = [stream('mic-a', { channels: 2 })];
    const after = [stream('mic-a', { channels: 2, bytes: 99_000 })];
    expect(musicVerdict(old, after, [{ id: 'mic-a', bytesBefore: 1000, bytesAfter: 99_000 }])).toBe(
      'no new audio stream',
    );
  });
});

import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setConverter } from '../../../convert';
import { parseProbeLog } from '../../../convert/probe';
import type { Converter } from '../../../convert/types';
import { MemoryLedger, type MountUploadRow } from '../../../lib/uploadLedger';
import { MountUploadManager, type MountUploadApi } from '../mountUploads';
import { UploadPanel } from '../UploadPanel';

const probe = parseProbeLog([
  "Input #0, matroska,webm, from 'x':",
  '  Duration: 00:00:10.00, start: 0.0, bitrate: 900 kb/s',
  '  Stream #0:0: Video: h264 (High), 1280x720, 25 fps',
  '  Stream #0:1(eng): Audio: aac (LC), 48000 Hz, stereo',
  '  Stream #0:2(eng): Subtitle: subrip',
]);

const converter: Converter = {
  probe: async () => probe,
  remux: async () => ({
    output: {
      file: new File([new Uint8Array([1])], 'Film.mp4'),
      name: 'Film.mp4',
      mime: 'video/mp4',
    },
    plan: { container: 'mp4', map: [0, 1], dropped: [], videoCodec: 'h264', audioCodecs: ['aac'] },
    probe,
  }),
  extractSubtitles: async () => ({
    subtitles: [
      {
        file: new File(['1'], 'Film.eng.srt'),
        name: 'Film.eng.srt',
        mime: 'application/x-subrip',
        format: 'srt',
        streamIndex: 2,
        language: 'eng',
      },
    ],
    skipped: [],
    probe,
  }),
  withInput: async () => {
    throw new Error('unused');
  },
  dispose: () => undefined,
};

const NO_TASKS: never[] = [];
function setup() {
  const enqueue = vi.fn(async () => undefined);
  const manager = {
    subscribe: () => () => undefined,
    snapshot: () => NO_TASKS,
    total: () => 1,
    resumeAll: async () => undefined,
    enqueue,
  } as unknown as MountUploadManager;
  void ({} as MountUploadApi);
  void new MemoryLedger<MountUploadRow>();
  render(
    <UploadPanel mountId="501" prefix="Movies/" manager={manager} onUploaded={() => undefined} />,
  );
  return enqueue;
}

// jsdom's canPlayType answers '' for everything; a browser plays H.264/AAC/Opus/VP9 but not HEVC or AC-3 here,
// which is what the WebCodecs decision table (reached through the bridge) asks.
beforeEach(() => {
  vi.spyOn(HTMLMediaElement.prototype, 'canPlayType').mockImplementation((m: string) =>
    /hvc1|hev1|ac-3|ec-3/.test(m) ? '' : 'probably',
  );
});
afterEach(() => setConverter(null));

describe('UploadPanel + Convert step (BYOS-CNV-001)', () => {
  it('uploads a web-native file straight away', async () => {
    setConverter(converter);
    const enqueue = setup();
    const user = userEvent.setup();
    await user.upload(
      screen.getByTestId('sto-pick-files'),
      new File([new Uint8Array(5)], 'clip.mp4', { type: 'video/mp4' }),
    );
    await waitFor(() =>
      expect(enqueue).toHaveBeenCalledWith('501', 'Movies/clip.mp4', expect.any(File)),
    );
    expect(screen.queryByText('Make these files playable')).toBeNull();
  });

  it('holds an MKV for the Convert step, then uploads the original, converted/<name>.mp4 and the subtitle', async () => {
    setConverter(converter);
    const enqueue = setup();
    const user = userEvent.setup();
    await user.upload(
      screen.getByTestId('sto-pick-files'),
      new File([new Uint8Array(5)], 'Film.mkv'),
    );
    expect(await screen.findByText('Make these files playable')).toBeInTheDocument();
    expect(enqueue).not.toHaveBeenCalled();
    await user.click(screen.getByRole('button', { name: 'Convert in this browser' }));
    await user.click(await screen.findByRole('button', { name: 'Continue' }));
    const keys = enqueue.mock.calls.map((c) => (c as unknown as string[])[1]).sort();
    expect(keys).toEqual([
      'Movies/Film.mkv',
      'converted/Movies/Film.eng.srt',
      'converted/Movies/Film.mp4',
    ]);
    expect(screen.queryByText('Make these files playable')).toBeNull();
  });

  it('"Upload as they are" sends only the original', async () => {
    setConverter(converter);
    const enqueue = setup();
    const user = userEvent.setup();
    await user.upload(
      screen.getByTestId('sto-pick-files'),
      new File([new Uint8Array(5)], 'Film.avi'),
    );
    await user.click(await screen.findByRole('button', { name: 'Upload as they are' }));
    expect(enqueue).toHaveBeenCalledTimes(1);
    expect(enqueue).toHaveBeenCalledWith('501', 'Movies/Film.avi', expect.any(File));
  });
});

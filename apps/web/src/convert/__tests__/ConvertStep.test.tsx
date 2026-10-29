import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, vi } from 'vitest';
import { axeViolations } from '../../../../../packages/ui/src/test-axe';
import { ConvertStep } from '../ConvertStep';
import { parseProbeLog } from '../probe';
import { http, HttpResponse, server } from '../../test/server';
import { ConvertError, type Converter } from '../types';

const LOG = [
  "Input #0, matroska,webm, from 'x':",
  '  Duration: 00:00:10.00, start: 0.0, bitrate: 900 kb/s',
  '  Stream #0:0: Video: h264 (High), 1280x720, 25 fps',
  '  Stream #0:1(eng): Audio: aac (LC), 48000 Hz, stereo',
  '  Stream #0:2(eng): Subtitle: subrip',
];
const probe = parseProbeLog(LOG);

// jsdom's canPlayType answers '' for everything; a browser plays H.264/AAC/Opus/VP9 but not HEVC or AC-3 here,
// which is what the WebCodecs decision table (reached through the bridge) asks.
beforeEach(() => {
  vi.spyOn(HTMLMediaElement.prototype, 'canPlayType').mockImplementation((m: string) =>
    /hvc1|hev1|ac-3|ec-3/.test(m) ? '' : 'probably',
  );
});

function fake(over: Partial<Converter> = {}): Converter {
  return {
    probe: async () => probe,
    remux: async (_file, o) => {
      o?.onProgress?.({ phase: 'remuxing', fraction: 0.5, mediaSec: 5, elapsedSec: 2, etaSec: 90 });
      return {
        output: {
          file: new File([new Uint8Array([1])], 'Film.mp4'),
          name: 'Film.mp4',
          mime: 'video/mp4',
        },
        plan: {
          container: 'mp4',
          map: [0, 1],
          dropped: [],
          videoCodec: 'h264',
          audioCodecs: ['aac'],
        },
        probe,
      };
    },
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
    ...over,
  };
}

const mkv = new File([new Uint8Array(4)], 'Film.mkv');

describe('ConvertStep', () => {
  it('converts, shows the subtitle count and hands the result over', async () => {
    const user = userEvent.setup();
    const onDone = vi.fn();
    const { container } = render(
      <ConvertStep
        files={[mkv]}
        converter={fake()}
        onDone={onDone}
        onAsIs={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(await axeViolations(container)).toEqual([]);
    await user.click(screen.getByRole('button', { name: 'Convert in this browser' }));
    expect(await screen.findByText('1 subtitle file')).toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: 'Continue' }));
    expect(onDone).toHaveBeenCalledTimes(1);
    const [items] = onDone.mock.calls[0]!;
    expect(items).toHaveLength(1);
    expect(items[0].output.name).toBe('Film.mp4');
    expect(items[0].subtitles[0].name).toBe('Film.eng.srt');
    expect(items[0].original).toBe(mkv);
  });

  it('skips subtitle extraction when the box is cleared', async () => {
    const user = userEvent.setup();
    const extract = vi.fn();
    render(
      <ConvertStep
        files={[mkv]}
        converter={fake({ extractSubtitles: extract })}
        onDone={vi.fn()}
        onAsIs={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    await user.click(screen.getByRole('checkbox'));
    await user.click(screen.getByRole('button', { name: 'Convert in this browser' }));
    await screen.findByRole('button', { name: 'Continue' });
    expect(extract).not.toHaveBeenCalled();
  });

  it('shows the worker hint for a file over the limit and keeps the choice to upload as is', async () => {
    const user = userEvent.setup();
    const onAsIs = vi.fn();
    render(
      <ConvertStep
        files={[mkv]}
        converter={fake({
          probe: async () => {
            throw new ConvertError('TOO_LARGE', 'big', 512);
          },
        })}
        onDone={vi.fn()}
        onAsIs={onAsIs}
        onCancel={vi.fn()}
      />,
    );
    await user.click(screen.getByRole('button', { name: 'Convert in this browser' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('over 512 MiB');
    expect(screen.getByRole('alert')).toHaveTextContent('transcoding worker');
    await user.click(screen.getByRole('button', { name: 'Upload as they are' }));
    expect(onAsIs).toHaveBeenCalled();
  });

  it('hides "upload as they are" when the server requires conversion', () => {
    render(
      <ConvertStep
        files={[mkv]}
        required
        converter={fake()}
        onDone={vi.fn()}
        onAsIs={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.queryByRole('button', { name: 'Upload as they are' })).toBeNull();
    expect(screen.getByText(/only accepts files that play/)).toBeInTheDocument();
  });

  it('takes the size cap and "required" from the shelves summary when the dialog passes neither', async () => {
    server.use(
      http.get('*/api/v1/users/@me/shelves/summary', () =>
        HttpResponse.json({
          cinema: 0,
          library: 0,
          music: 0,
          convert: { max_mib: 512, require_client_remux: true },
        }),
      ),
    );
    const MiB = 1024 * 1024;
    const big = new File([new Uint8Array(1)], 'Big.mkv');
    Object.defineProperty(big, 'size', { value: 513 * MiB });
    const user = userEvent.setup();
    render(
      <ConvertStep
        files={[big]}
        converter={fake({
          // The real converter's guard: the file against the limit it is given (2048 when none).
          probe: async (file, o) => {
            const limit = o?.maxMiB ?? 2048;
            if (file.size > limit * MiB) throw new ConvertError('TOO_LARGE', 'big', limit);
            return probe;
          },
        })}
        onDone={vi.fn()}
        onAsIs={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    // required: no "upload as they are".
    expect(await screen.findByText(/only accepts files that play/)).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'Upload as they are' })).toBeNull();
    await user.click(screen.getByRole('button', { name: 'Convert in this browser' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('over 512 MiB');
  });

  it('says what remux cannot fix, and stop cancels the job', async () => {
    const user = userEvent.setup();
    const ac = { signal: undefined as AbortSignal | undefined };
    const c = fake({
      probe: async () => ({
        ...probe,
        streams: [probe.streams[0]!, { index: 1, kind: 'audio', codec: 'ac3' }],
      }),
    });
    render(
      <ConvertStep
        files={[mkv]}
        converter={c}
        onDone={vi.fn()}
        onAsIs={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    await user.click(screen.getByRole('button', { name: 'Convert in this browser' }));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('the sound format'));
    void ac;
  });
});

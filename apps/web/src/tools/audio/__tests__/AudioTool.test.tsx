import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

const RATE = 8000;
const data = new Float32Array(RATE * 4);
for (let i = 0; i < data.length; i++) data[i] = 0.1 * Math.sin((2 * Math.PI * 440 * i) / RATE);

vi.mock('../pcm', async (orig) => ({
  ...(await orig<object>()),
  decodeAudioFile: async () => ({
    sampleRate: RATE,
    length: data.length,
    numberOfChannels: 1,
    getChannelData: () => data,
  }),
}));

const { encodeOpus } = vi.hoisted(() => ({
  encodeOpus: vi.fn(async () => new Blob([new Uint8Array(2000)], { type: 'audio/webm' })),
}));
vi.mock('../dsp', async (orig) => ({ ...(await orig<object>()), encodeOpus }));

import { AudioTool } from '../AudioTool';

describe('AudioTool (UTL-AUD-001)', () => {
  it('caps the kept part at the embedded limit and hands back a WAV that fits the size rule', async () => {
    const onUse = vi.fn();
    render(
      <AudioTool
        file={new File(['x'], 'tone.mp3', { type: 'audio/mpeg' })}
        limits={{ maxSeconds: 1, maxBytes: 512 * 1024, format: 'wav' }}
        onUse={onUse}
      />,
    );
    await waitFor(() => expect(screen.getByTestId('audio-range')).toBeTruthy());
    expect(screen.getByTestId('audio-range').textContent).toContain('1.00 s kept');
    // Dragging the end handle past the limit pushes the start up so the part stays within it.
    const [startHandle, endHandle] = screen.getAllByRole('slider');
    fireEvent.change(endHandle!, { target: { value: '3' } });
    expect(screen.getByTestId('audio-range').textContent).toContain('1.00 s kept');
    expect((startHandle as HTMLInputElement).value).toBe('2');
  });

  it('slides the window when the start handle moves forward at the limit (D-533)', async () => {
    render(
      <AudioTool
        file={new File(['x'], 'tone.mp3', { type: 'audio/mpeg' })}
        limits={{ maxSeconds: 1, maxBytes: 512 * 1024, format: 'wav' }}
        onUse={vi.fn()}
      />,
    );
    await waitFor(() => expect(screen.getByTestId('audio-range')).toBeTruthy());
    const [startHandle, endHandle] = screen.getAllByRole('slider') as HTMLInputElement[];
    fireEvent.change(startHandle!, { target: { value: '2' } });
    expect(endHandle!.value).toBe('3');
    expect(screen.getByTestId('audio-range').textContent).toContain('1.00 s kept');
    // Back to the start: the part may not exceed the limit, so the end follows it down.
    fireEvent.change(startHandle!, { target: { value: '0' } });
    expect(endHandle!.value).toBe('1');
    // At the end of the file the window stops sliding and the part shrinks.
    fireEvent.change(startHandle!, { target: { value: '3.5' } });
    expect(Number(endHandle!.value)).toBe(4);
  });

  it('saves a soundboard part as Opus where the browser records it (D-534)', async () => {
    vi.stubGlobal('MediaRecorder', {
      isTypeSupported: (t: string) => t === 'audio/webm;codecs=opus',
    });
    const onUse = vi.fn();
    render(
      <AudioTool
        file={new File(['x'], 'tone.mp3', { type: 'audio/mpeg' })}
        limits={{ maxSeconds: 5, maxBytes: 512 * 1024, format: 'opus' }}
        onUse={onUse}
      />,
    );
    await waitFor(() => expect(screen.getByTestId('audio-format')).toBeTruthy());
    expect(screen.getByTestId('audio-format').textContent).toContain('saved as Opus');
    fireEvent.click(screen.getByRole('button', { name: 'Use this' }));
    await waitFor(() => expect(onUse).toHaveBeenCalled());
    const made = onUse.mock.calls[0]![0] as File;
    expect(made.name).toBe('tone-trimmed.webm');
    expect(made.type).toBe('audio/webm');
    vi.unstubAllGlobals();
  });

  it('prefers WebM for soundboard sounds and Ogg for voice messages (D-551)', async () => {
    vi.stubGlobal('MediaRecorder', { isTypeSupported: () => true });
    const sound = vi.fn();
    const { unmount } = render(
      <AudioTool
        file={new File(['x'], 'tone.mp3', { type: 'audio/mpeg' })}
        limits={{ maxSeconds: 5, maxBytes: 512 * 1024, format: 'opus' }}
        onUse={sound}
      />,
    );
    await waitFor(() => expect(screen.getByTestId('audio-format')).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: 'Use this' }));
    await waitFor(() => expect(sound).toHaveBeenCalled());
    expect((sound.mock.calls[0]![0] as File).name).toBe('tone-trimmed.webm');
    unmount();
    const voice = vi.fn();
    render(
      <AudioTool
        file={new File(['x'], 'tone.mp3', { type: 'audio/mpeg' })}
        limits={{ maxSeconds: 60, format: 'ogg' }}
        onUse={voice}
      />,
    );
    await waitFor(() => expect(screen.getAllByRole('button', { name: 'Use this' }).length).toBe(1));
    fireEvent.click(screen.getByRole('button', { name: 'Use this' }));
    await waitFor(() => expect(voice).toHaveBeenCalled());
    expect((voice.mock.calls[0]![0] as File).name).toBe('tone-trimmed.ogg');
    vi.unstubAllGlobals();
  });

  it('saves Ogg for a soundboard part when only Ogg can be recorded (D-551)', async () => {
    vi.stubGlobal('MediaRecorder', { isTypeSupported: (t: string) => t.startsWith('audio/ogg') });
    const onUse = vi.fn();
    render(
      <AudioTool
        file={new File(['x'], 'tone.mp3', { type: 'audio/mpeg' })}
        limits={{ maxSeconds: 5, maxBytes: 512 * 1024, format: 'opus' }}
        onUse={onUse}
      />,
    );
    await waitFor(() => expect(screen.getByTestId('audio-format')).toBeTruthy());
    fireEvent.click(screen.getByRole('button', { name: 'Use this' }));
    await waitFor(() => expect(onUse).toHaveBeenCalled());
    expect((onUse.mock.calls[0]![0] as File).name).toBe('tone-trimmed.ogg');
    vi.unstubAllGlobals();
  });

  it('falls back to WAV for a soundboard part when the browser cannot record Opus (D-534)', async () => {
    vi.stubGlobal('MediaRecorder', undefined);
    const onUse = vi.fn();
    render(
      <AudioTool
        file={new File(['x'], 'tone.mp3', { type: 'audio/mpeg' })}
        limits={{ maxSeconds: 5, maxBytes: 512 * 1024, format: 'opus' }}
        onUse={onUse}
      />,
    );
    await waitFor(() => expect(screen.getByTestId('audio-format')).toBeTruthy());
    expect(screen.getByTestId('audio-format').textContent).toContain('saved as WAV');
    fireEvent.click(screen.getByRole('button', { name: 'Use this' }));
    await waitFor(() => expect(onUse).toHaveBeenCalled());
    expect((onUse.mock.calls[0]![0] as File).name).toBe('tone-trimmed.wav');
    vi.unstubAllGlobals();
  });
});

describe('AudioTool Cancel (XC-VIS-004)', () => {
  const file = () => new File(['x'], 'tone.mp3', { type: 'audio/mpeg' });
  it('shows its own Cancel by default', async () => {
    render(<AudioTool file={file()} onUse={vi.fn()} onCancel={vi.fn()} />);
    await waitFor(() => expect(screen.getByTestId('audio-range')).toBeTruthy());
    expect(screen.getByRole('button', { name: 'Cancel' })).toBeTruthy();
  });
  it('hides it when the host dialog has its own Cancel', async () => {
    render(<AudioTool file={file()} onUse={vi.fn()} onCancel={vi.fn()} hideCancel />);
    await waitFor(() => expect(screen.getByTestId('audio-range')).toBeTruthy());
    expect(screen.queryByRole('button', { name: 'Cancel' })).toBeNull();
  });
});

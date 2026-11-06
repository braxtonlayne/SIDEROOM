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

/**
 * D-1050: the AudioWorklet that turns the microphone into 16 kHz mono frames for the detector. It runs on the audio
 * thread inside the page's own context; the frames go to its port and nowhere else. Loaded with
 * `audioWorklet.addModule(<url of this file's own chunk>)` from the same origin (`script-src 'self'`).
 * The mean of the input channels is taken without allocating; the only allocation is the frame it hands on.
 */
import { FRAME_SAMPLES, SAMPLE_RATE } from './constants';
import { FrameResampler } from './resampler';

declare const sampleRate: number;
declare class AudioWorkletProcessor {
  readonly port: MessagePort;
  constructor(options?: unknown);
}
declare function registerProcessor(
  name: string,
  ctor: new (options?: unknown) => AudioWorkletProcessor,
): void;

class SileroFrames extends AudioWorkletProcessor {
  private readonly resampler = new FrameResampler(sampleRate, SAMPLE_RATE, FRAME_SAMPLES, (frame) =>
    this.port.postMessage(frame, [frame.buffer]),
  );
  private mono: Float32Array | null = null;

  process(inputs: Float32Array[][]): boolean {
    const channels = inputs[0];
    if (!channels || channels.length === 0 || !channels[0]) return true;
    const first = channels[0];
    if (channels.length === 1) {
      this.resampler.push(first);
      return true;
    }
    if (!this.mono || this.mono.length !== first.length) this.mono = new Float32Array(first.length);
    const mono = this.mono;
    const count = channels.length;
    for (let i = 0; i < first.length; i++) {
      let sum = 0;
      for (let c = 0; c < count; c++) sum += channels[c]![i] ?? 0;
      mono[i] = sum / count;
    }
    this.resampler.push(mono);
    return true;
  }
}

registerProcessor('sr-silero-frames', SileroFrames);

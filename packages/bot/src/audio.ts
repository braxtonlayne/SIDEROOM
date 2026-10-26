// An Opus encoder and decoder for bots in voice (DEV-FU-002, D-377, D-378): the TypeScript twin of
// `sr_sdk::audio` (crates/sr-sdk/src/audio.rs), with the same names. `opusscript` (libopus compiled to
// WebAssembly, no native build) does the coding. `sendOpus` still takes any pre-encoded packet, so a bot that
// already has Opus does not need this module.
//
// WebAssembly memory is not garbage collected: every encoder and decoder must be `close()`d. Hand them to the
// session (`VoiceSession.adopt`) and `leave()` does it.
import OpusScript from 'opusscript';

/** The fixed sample rate of every frame, in hertz. */
export const SAMPLE_RATE = 48_000;
/** Samples per channel in one 20 ms frame. */
export const FRAME_SAMPLES = 960;

export type Channels = 1 | 2;
export type Application = 'voip' | 'audio' | 'lowdelay';

const APPLICATION: Record<Application, number> = {
  voip: OpusScript.Application.VOIP,
  audio: OpusScript.Application.AUDIO,
  lowdelay: OpusScript.Application.RESTRICTED_LOWDELAY,
};

/** libopus request numbers (opus_defines.h) for the two settings opusscript has no method for. */
const OPUS_SET_INBAND_FEC = 4012;

/** A wrong frame length; never padded or cut silently. */
export class AudioError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'AudioError';
  }
}

/** Anything with WebAssembly memory to give back; `VoiceSession.adopt` closes these on leave. */
export interface Closeable {
  close(): void;
}

/** Encodes 20 ms frames of 48 kHz 16-bit PCM into Opus packets. */
export class OpusEncoder implements Closeable {
  private codec: OpusScript | null;
  readonly channels: Channels;

  /** Always the WebAssembly build (`wasm: true`): no native module is loaded. */
  constructor(channels: Channels = 1, application: Application = 'voip') {
    this.channels = channels;
    this.codec = new OpusScript(SAMPLE_RATE, channels, APPLICATION[application], { wasm: true });
  }

  /** Samples (all channels, interleaved) that `encodeFrame` takes. */
  get frameLen(): number {
    return FRAME_SAMPLES * this.channels;
  }

  /** Encode exactly one 20 ms frame: 960 samples per channel, interleaved for stereo. */
  encodeFrame(pcm: Int16Array): Uint8Array {
    const codec = this.live();
    if (pcm.length !== this.frameLen) {
      throw new AudioError(
        `a frame is ${this.frameLen} samples (${pcm.length} given): exactly 20 ms at 48 kHz`,
      );
    }
    const bytes = Buffer.from(pcm.buffer, pcm.byteOffset, pcm.byteLength);
    const packet = codec.encode(bytes, FRAME_SAMPLES);
    // Own the bytes: opusscript hands back a view it may reuse.
    return Uint8Array.from(packet);
  }

  /** Target bitrate in bits per second. */
  setBitrate(bitsPerSecond: number): void {
    this.live().setBitrate(bitsPerSecond);
  }

  /** Put a low-bitrate copy of the previous frame into each packet so a receiver can repair one lost packet. */
  setInbandFec(on: boolean): void {
    this.live().encoderCTL(OPUS_SET_INBAND_FEC, on ? 1 : 0);
  }

  /** Free the WebAssembly memory (`delete()`); safe to call twice. */
  close(): void {
    this.codec?.delete();
    this.codec = null;
  }

  private live(): OpusScript {
    if (!this.codec) throw new AudioError('the encoder is closed');
    return this.codec;
  }
}

/** Decodes Opus packets into 48 kHz 16-bit PCM. */
export class OpusDecoder implements Closeable {
  private codec: OpusScript | null;
  readonly channels: Channels;

  constructor(channels: Channels = 1) {
    this.channels = channels;
    this.codec = new OpusScript(SAMPLE_RATE, channels, OpusScript.Application.AUDIO, {
      wasm: true,
    });
  }

  /**
   * Decode one packet into interleaved samples. A packet libopus rejects decodes to an empty buffer (a bot
   * hearing garbage hears silence, it does not die). opusscript 0.1.1 exposes no in-band FEC decoding or loss
   * concealment, so unlike the Rust `decode_frame` there is no `fec` flag.
   */
  decodeFrame(packet: Uint8Array): Int16Array {
    if (!this.codec) throw new AudioError('the decoder is closed');
    try {
      const out = this.codec.decode(
        Buffer.from(packet.buffer, packet.byteOffset, packet.byteLength),
      );
      return new Int16Array(out.buffer.slice(out.byteOffset, out.byteOffset + out.byteLength));
    } catch {
      return new Int16Array(0);
    }
  }

  /** Free the WebAssembly memory (`delete()`); safe to call twice. */
  close(): void {
    this.codec?.delete();
    this.codec = null;
  }
}

/**
 * The level of a frame in dBov for an RFC 6464 audio-level field: 0 is full scale, -127 is silence. Computed
 * from the RMS of the samples.
 */
export function audioLevelDbov(pcm: Int16Array): number {
  if (pcm.length === 0) return -127;
  let sum = 0;
  for (const s of pcm) sum += s * s;
  const rms = Math.sqrt(sum / pcm.length) / 32768;
  if (rms <= 0) return -127;
  // `+ 0` turns a rounded -0 into 0.
  return Math.max(-127, Math.min(0, Math.round(20 * Math.log10(rms)))) + 0;
}

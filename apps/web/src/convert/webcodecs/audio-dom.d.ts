/* eslint-disable no-var */
// TypeScript 5.6's lib.dom has the video half of WebCodecs but not the audio half.
// These are the parts of the W3C WebCodecs spec the pipeline uses. Remove this
// file once the repo's TypeScript ships them (a duplicate `declare var` will say so).

type AudioSampleFormat =
  'u8' | 'u8-planar' | 's16' | 's16-planar' | 's32' | 's32-planar' | 'f32' | 'f32-planar';
type EncodedAudioChunkType = 'key' | 'delta';

interface AudioDataInit {
  format: AudioSampleFormat;
  sampleRate: number;
  numberOfFrames: number;
  numberOfChannels: number;
  timestamp: number;
  data: BufferSource;
}
interface AudioDataCopyToOptions {
  planeIndex: number;
  frameOffset?: number;
  frameCount?: number;
  format?: AudioSampleFormat;
}
interface AudioData {
  readonly format: AudioSampleFormat | null;
  readonly sampleRate: number;
  readonly numberOfFrames: number;
  readonly numberOfChannels: number;
  readonly duration: number;
  readonly timestamp: number;
  allocationSize(options: AudioDataCopyToOptions): number;
  copyTo(destination: BufferSource, options: AudioDataCopyToOptions): void;
  close(): void;
}
declare var AudioData: {
  prototype: AudioData;
  new (init: AudioDataInit): AudioData;
};

interface EncodedAudioChunkInit {
  type: EncodedAudioChunkType;
  timestamp: number;
  duration?: number;
  data: BufferSource;
}
interface EncodedAudioChunk {
  readonly type: EncodedAudioChunkType;
  readonly timestamp: number;
  readonly duration: number | null;
  readonly byteLength: number;
  copyTo(destination: BufferSource): void;
}
declare var EncodedAudioChunk: {
  prototype: EncodedAudioChunk;
  new (init: EncodedAudioChunkInit): EncodedAudioChunk;
};

interface AudioDecoderConfig {
  codec: string;
  sampleRate: number;
  numberOfChannels: number;
  description?: BufferSource;
}
interface AudioDecoderSupport {
  supported?: boolean;
  config?: AudioDecoderConfig;
}
interface AudioDecoderInit {
  output: (output: AudioData) => void;
  error: (error: DOMException) => void;
}
interface AudioDecoder extends EventTarget {
  readonly state: 'unconfigured' | 'configured' | 'closed';
  readonly decodeQueueSize: number;
  configure(config: AudioDecoderConfig): void;
  decode(chunk: EncodedAudioChunk): void;
  flush(): Promise<void>;
  reset(): void;
  close(): void;
}
declare var AudioDecoder: {
  prototype: AudioDecoder;
  new (init: AudioDecoderInit): AudioDecoder;
  isConfigSupported(config: AudioDecoderConfig): Promise<AudioDecoderSupport>;
};

interface AacEncoderConfig {
  format?: 'aac' | 'adts';
}
interface OpusEncoderConfig {
  format?: 'opus' | 'ogg';
  frameDuration?: number;
  complexity?: number;
  packetlossperc?: number;
  useinbandfec?: boolean;
  usedtx?: boolean;
}
interface AudioEncoderConfig {
  codec: string;
  sampleRate: number;
  numberOfChannels: number;
  bitrate?: number;
  bitrateMode?: 'constant' | 'variable';
  aac?: AacEncoderConfig;
  opus?: OpusEncoderConfig;
}
interface AudioEncoderSupport {
  supported?: boolean;
  config?: AudioEncoderConfig;
}
interface EncodedAudioChunkMetadata {
  decoderConfig?: AudioDecoderConfig;
}
interface AudioEncoderInit {
  output: (output: EncodedAudioChunk, metadata?: EncodedAudioChunkMetadata) => void;
  error: (error: DOMException) => void;
}
interface AudioEncoder extends EventTarget {
  readonly state: 'unconfigured' | 'configured' | 'closed';
  readonly encodeQueueSize: number;
  configure(config: AudioEncoderConfig): void;
  encode(data: AudioData): void;
  flush(): Promise<void>;
  reset(): void;
  close(): void;
}
declare var AudioEncoder: {
  prototype: AudioEncoder;
  new (init: AudioEncoderInit): AudioEncoder;
  isConfigSupported(config: AudioEncoderConfig): Promise<AudioEncoderSupport>;
};

// BYOS-CNV-002: the WebCodecs pipeline. Packets from the fMP4 demuxer go
//   video: VideoDecoder -> (scale) -> VideoEncoder -> muxer      (or copied)
//   audio: AudioDecoder -> (resample/downmix) -> AudioEncoder -> muxer (or copied)
// with back-pressure on both queues, abort support, and every WebCodecs
// failure reported as a WebCodecsError. All platform access goes through
// `WebCodecsApi`, so the Vitest suite drives it with fakes.

import { codecFamily } from '../decide';
import { convertPlanes, planesToAudioData, readPlanes } from './audio';
import { createMuxer, outputMime } from './muxers';
import type {
  MuxerHandle,
  Packet,
  TranscodeOptions,
  TranscodeResult,
  VideoPlan,
  WebCodecsApi,
} from './types';
import { WebCodecsError } from './types';

const MAX_QUEUE = 8;
const KEYFRAME_SECONDS = 2;

type VideoMuxCodec = 'avc' | 'hevc' | 'vp9' | 'av1' | 'vp8';

export function defaultApi(): WebCodecsApi {
  let canvas: OffscreenCanvas | null = null;
  let ctx: OffscreenCanvasRenderingContext2D | null = null;
  return {
    VideoDecoder,
    VideoEncoder,
    AudioDecoder,
    AudioEncoder,
    EncodedVideoChunk,
    EncodedAudioChunk,
    AudioData,
    createMuxer,
    scaleFrame(frame, width, height) {
      if (!canvas || canvas.width !== width || canvas.height !== height) {
        canvas = new OffscreenCanvas(width, height);
        ctx = canvas.getContext('2d');
      }
      if (!ctx) throw new WebCodecsError('no 2D canvas to scale frames with', 'unsupported');
      ctx.drawImage(frame, 0, 0, width, height);
      return new VideoFrame(canvas, {
        timestamp: frame.timestamp,
        duration: frame.duration ?? undefined,
      });
    },
  };
}

const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

function muxVideoCodec(plan: VideoPlan, sourceCodec: string): VideoMuxCodec {
  if (plan.mode === 'transcode') return plan.to;
  const fam = codecFamily(sourceCodec);
  if (fam === 'avc' || fam === 'hevc' || fam === 'vp8' || fam === 'vp9' || fam === 'av1')
    return fam;
  throw new WebCodecsError(`cannot copy ${sourceCodec} into a new file`, 'mux');
}

/** Runs the plan `decide()` returned. Resolves with the finished file, or throws WebCodecsError. */
export async function transcodeWithWebCodecs(opts: TranscodeOptions): Promise<TranscodeResult> {
  const { source, plan, signal } = opts;
  const api = opts.api ?? defaultApi();
  const output = opts.output ?? { kind: 'memory' as const };
  const vPlan = plan.video;
  const aPlan = plan.audio;
  if (vPlan && !source.video)
    throw new WebCodecsError('plan wants video but the source has none', 'unsupported');
  if (aPlan.mode !== 'none' && !source.audio) {
    throw new WebCodecsError('plan wants audio but the source has none', 'unsupported');
  }

  let failure: WebCodecsError | null = null;
  const fail = (code: WebCodecsError['code'], e: unknown) => {
    failure ??= new WebCodecsError(e instanceof Error ? e.message : String(e), code);
  };
  const check = () => {
    if (signal?.aborted) throw new WebCodecsError('conversion cancelled', 'aborted');
    if (failure) throw failure;
  };

  const fps = Math.min(120, Math.max(1, Math.round(source.video?.frameRate ?? 30)));

  const mux: MuxerHandle = await api.createMuxer({
    container: plan.container,
    video:
      vPlan && source.video
        ? {
            codec: muxVideoCodec(vPlan, source.video.config.codec),
            width: vPlan.mode === 'transcode' ? vPlan.width : source.video.width,
            height: vPlan.mode === 'transcode' ? vPlan.height : source.video.height,
            frameRate: fps,
          }
        : undefined,
    audio:
      aPlan.mode === 'none' || !source.audio
        ? undefined
        : aPlan.mode === 'transcode'
          ? { codec: aPlan.to, sampleRate: aPlan.sampleRate, channels: aPlan.channels }
          : {
              codec: codecFamily(aPlan.codec) === 'opus' ? 'opus' : 'aac',
              sampleRate: source.audio.sampleRate,
              channels: source.audio.channels,
            },
    output,
  });

  let videoFrames = 0;
  let audioFrames = 0;
  let videoIn = 0;
  let sentVideoMeta = false;
  let sentAudioMeta = false;
  const duration = Math.max(0.001, source.durationSec);
  let lastReport = 0;
  const report = (tsUs: number) => {
    const now = Date.now();
    if (now - lastReport < 100 && tsUs / 1e6 < duration) return;
    lastReport = now;
    opts.onProgress?.({ fraction: Math.min(1, tsUs / 1e6 / duration), videoFrames, audioFrames });
  };

  // ---- video ---------------------------------------------------------------
  let vDecoder: VideoDecoder | null = null;
  let vEncoder: VideoEncoder | null = null;
  if (vPlan?.mode === 'transcode' && source.video) {
    const { width, height, step } = vPlan;
    const keyEvery = Math.round(fps * KEYFRAME_SECONDS);
    vEncoder = new api.VideoEncoder({
      output: (chunk, meta) => {
        try {
          mux.addVideo(chunk, meta);
          videoFrames++;
        } catch (e) {
          fail('mux', e);
        }
      },
      error: (e) => fail('encode', e),
    });
    const encoderConfig: VideoEncoderConfig = {
      codec: vPlan.codec,
      width,
      height,
      bitrate: step.videoBitrate,
      framerate: fps,
      latencyMode: 'quality',
    };
    if (vPlan.to === 'avc') encoderConfig.avc = { format: 'avc' };
    vEncoder.configure(encoderConfig);
    const enc = vEncoder;
    vDecoder = new api.VideoDecoder({
      output: (frame) => {
        try {
          const needsScale = frame.displayWidth !== width || frame.displayHeight !== height;
          const out = needsScale ? api.scaleFrame(frame, width, height) : frame;
          enc.encode(out, { keyFrame: videoIn % keyEvery === 0 });
          videoIn++;
          if (out !== frame) out.close();
        } catch (e) {
          fail('encode', e);
        } finally {
          frame.close();
        }
      },
      error: (e) => fail('decode', e),
    });
    vDecoder.configure(source.video.config);
  }

  // ---- audio ---------------------------------------------------------------
  let aDecoder: AudioDecoder | null = null;
  let aEncoder: AudioEncoder | null = null;
  if (aPlan.mode === 'transcode' && source.audio) {
    const { sampleRate, channels, bitrate, codec } = aPlan;
    const srcRate = source.audio.sampleRate;
    aEncoder = new api.AudioEncoder({
      output: (chunk, meta) => {
        try {
          mux.addAudio(chunk, meta);
          audioFrames++;
        } catch (e) {
          fail('mux', e);
        }
      },
      error: (e) => fail('encode', e),
    });
    aEncoder.configure({ codec, sampleRate, numberOfChannels: channels, bitrate });
    const enc = aEncoder;
    aDecoder = new api.AudioDecoder({
      output: (data) => {
        try {
          if (data.sampleRate === sampleRate && data.numberOfChannels === channels) {
            enc.encode(data);
          } else {
            const planes = convertPlanes(
              readPlanes(data),
              data.sampleRate || srcRate,
              sampleRate,
              channels as 1 | 2,
            );
            const converted = planesToAudioData(api.AudioData, planes, sampleRate, data.timestamp);
            enc.encode(converted);
            converted.close();
          }
        } catch (e) {
          fail('encode', e);
        } finally {
          data.close();
        }
      },
      error: (e) => fail('decode', e),
    });
    aDecoder.configure(source.audio.config);
  }

  const closeAll = () => {
    for (const c of [vDecoder, vEncoder, aDecoder, aEncoder]) {
      try {
        if (c && c.state !== 'closed') c.close();
      } catch {
        // already closed
      }
    }
  };

  const feed = async (p: Packet) => {
    check();
    if (p.kind === 'video' && source.video) {
      if (vDecoder && vEncoder) {
        while (vDecoder.decodeQueueSize > MAX_QUEUE || vEncoder.encodeQueueSize > MAX_QUEUE) {
          await sleep(1);
          check();
        }
        vDecoder.decode(
          new api.EncodedVideoChunk({
            type: p.key ? 'key' : 'delta',
            timestamp: p.timestampUs,
            duration: p.durationUs,
            data: p.data,
          }),
        );
      } else if (vPlan?.mode === 'copy') {
        const meta: EncodedVideoChunkMetadata | undefined = sentVideoMeta
          ? undefined
          : { decoderConfig: source.video.config };
        sentVideoMeta = true;
        mux.addVideo(
          new api.EncodedVideoChunk({
            type: p.key ? 'key' : 'delta',
            timestamp: p.timestampUs,
            duration: p.durationUs,
            data: p.data,
          }),
          meta,
        );
        videoFrames++;
      }
    } else if (p.kind === 'audio' && source.audio) {
      if (aDecoder && aEncoder) {
        while (
          aDecoder.decodeQueueSize > MAX_QUEUE * 4 ||
          aEncoder.encodeQueueSize > MAX_QUEUE * 4
        ) {
          await sleep(1);
          check();
        }
        aDecoder.decode(
          new api.EncodedAudioChunk({
            type: 'key',
            timestamp: p.timestampUs,
            duration: p.durationUs,
            data: p.data,
          }),
        );
      } else if (aPlan.mode === 'copy') {
        const meta: EncodedAudioChunkMetadata | undefined = sentAudioMeta
          ? undefined
          : { decoderConfig: source.audio.config };
        sentAudioMeta = true;
        mux.addAudio(
          new api.EncodedAudioChunk({
            type: 'key',
            timestamp: p.timestampUs,
            duration: p.durationUs,
            data: p.data,
          }),
          meta,
        );
        audioFrames++;
      }
    }
    report(p.timestampUs);
  };

  try {
    for await (const p of source.packets) await feed(p);
    check();
    await Promise.all([vDecoder?.flush(), aDecoder?.flush()]);
    check();
    await Promise.all([vEncoder?.flush(), aEncoder?.flush()]);
    check();
    const blob = mux.finalize();
    opts.onProgress?.({ fraction: 1, videoFrames, audioFrames });
    return {
      container: plan.container,
      mime: outputMime(plan.container, !!vPlan),
      blob,
      videoFrames,
      audioFrames,
    };
  } catch (e) {
    if (e instanceof WebCodecsError) throw e;
    throw failure ?? new WebCodecsError(e instanceof Error ? e.message : String(e), 'decode');
  } finally {
    closeAll();
  }
}

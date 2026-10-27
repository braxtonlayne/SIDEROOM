// BYOS-CNV-002: adapters over mp4-muxer 5.2.1 and webm-muxer 5.0.3 (both MIT,
// see THIRD_PARTY_NOTICES.md). Both libraries are imported lazily so that the
// WebCodecs path costs nothing until a member converts a file.

import type { MuxerHandle, MuxerSetup } from './types';
import { WebCodecsError } from './types';

const MIME = {
  mp4: { video: 'video/mp4', audio: 'audio/mp4' },
  webm: { video: 'video/webm', audio: 'audio/webm' },
} as const;

const WEBM_VIDEO: Record<string, string> = { vp8: 'V_VP8', vp9: 'V_VP9', av1: 'V_AV1' };
const WEBM_AUDIO: Record<string, string> = { opus: 'A_OPUS' };

export function outputMime(container: 'mp4' | 'webm', hasVideo: boolean): string {
  return MIME[container][hasVideo ? 'video' : 'audio'];
}

export async function createMuxer(setup: MuxerSetup): Promise<MuxerHandle> {
  const memory = setup.output.kind === 'memory';
  const hasVideo = !!setup.video;
  // The libraries insist on a 2-argument callback (position matters), so wrap whatever we were given.
  const sink = (data: Uint8Array, position: number) => {
    if (setup.output.kind === 'stream') setup.output.onData(data, position);
  };
  if (setup.container === 'mp4') {
    const { Muxer, ArrayBufferTarget, StreamTarget } = await import('mp4-muxer');
    if (setup.video?.codec === 'vp8') throw new WebCodecsError('VP8 cannot go into MP4', 'mux');
    const target = memory
      ? new ArrayBufferTarget()
      : new StreamTarget({
          onData: (data, position) => sink(data, position),
        });
    const muxer = new Muxer({
      target,
      video: setup.video && {
        codec: setup.video.codec as 'avc' | 'hevc' | 'vp9' | 'av1',
        width: setup.video.width,
        height: setup.video.height,
        frameRate: setup.video.frameRate,
      },
      audio: setup.audio && {
        codec: setup.audio.codec,
        sampleRate: setup.audio.sampleRate,
        numberOfChannels: setup.audio.channels,
      },
      fastStart: memory ? 'in-memory' : 'fragmented',
      firstTimestampBehavior: 'cross-track-offset',
    });
    return {
      addVideo: (c, m) => muxer.addVideoChunk(c, m),
      addAudio: (c, m) => muxer.addAudioChunk(c, m),
      finalize() {
        muxer.finalize();
        return memory
          ? new Blob([(target as InstanceType<typeof ArrayBufferTarget>).buffer], {
              type: outputMime('mp4', hasVideo),
            })
          : null;
      },
    };
  }

  const { Muxer, ArrayBufferTarget, StreamTarget } = await import('webm-muxer');
  const vcodec = setup.video && WEBM_VIDEO[setup.video.codec];
  if (setup.video && !vcodec)
    throw new WebCodecsError(`${setup.video.codec} cannot go into WebM`, 'mux');
  if (setup.audio && !WEBM_AUDIO[setup.audio.codec]) {
    throw new WebCodecsError(`${setup.audio.codec} cannot go into WebM`, 'mux');
  }
  const target = memory
    ? new ArrayBufferTarget()
    : new StreamTarget({
        onData: (data, position) => sink(data, position),
      });
  const muxer = new Muxer({
    target,
    video: setup.video && {
      codec: vcodec!,
      width: setup.video.width,
      height: setup.video.height,
      frameRate: setup.video.frameRate,
    },
    audio: setup.audio && {
      codec: WEBM_AUDIO[setup.audio.codec]!,
      sampleRate: setup.audio.sampleRate,
      numberOfChannels: setup.audio.channels,
    },
    streaming: !memory,
    firstTimestampBehavior: 'offset',
  });
  return {
    addVideo: (c, m) => muxer.addVideoChunk(c, m),
    addAudio: (c, m) => muxer.addAudioChunk(c, m),
    finalize() {
      muxer.finalize();
      return memory
        ? new Blob([(target as InstanceType<typeof ArrayBufferTarget>).buffer], {
            type: outputMime('webm', hasVideo),
          })
        : null;
    },
  };
}

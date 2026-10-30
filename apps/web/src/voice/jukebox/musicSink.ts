// MUS-JB-001 + MUS-JB-002 (wave-31 integration): the DJ's side of stream mode. The jukebox strip's `<audio>`
// element is routed through Web Audio once (MediaElementSource -> destination, so the DJ keeps hearing it), and
// while this member is the DJ of a stream-mode session the source is also tapped into a
// MediaStreamAudioDestination (`tapMusicForVoice`) whose stream the voice controller publishes as the member's
// music track: a second audio sender, stereo Opus on its own payload type, under the same frame encryptor as the
// microphone (D-249). Stop, a hand-over, leaving the jukebox or unmounting the strip unpublishes and detaches.
import { voiceController } from '../controller';
import { tapMusicForVoice, type MusicTap } from '../playback';
import type { MusicSink } from './controller';

/** What the sink needs from the voice controller (a fake in tests). */
export interface MusicPublisher {
  publishMusic(stream: MediaStream): void;
  unpublishMusic(): void;
}

export interface BrowserMusicSink extends MusicSink {
  /** Unmount: stop and close the audio context (the element is going away with it). */
  dispose(): void;
}

function newContext(): AudioContext | null {
  if (typeof window === 'undefined') return null;
  const Ctor =
    window.AudioContext ??
    (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
  return Ctor ? new Ctor() : null;
}

export function browserMusicSink(
  audio: HTMLAudioElement,
  publisher: MusicPublisher = voiceController(),
  makeContext: () => AudioContext | null = newContext,
): BrowserMusicSink {
  let ctx: AudioContext | null = null;
  let source: AudioNode | null = null;
  let tap: MusicTap | null = null;
  const stop = () => {
    if (!tap) return;
    publisher.unpublishMusic();
    tap.detach();
    tap = null;
  };
  return {
    start() {
      if (tap) return;
      if (!source) {
        ctx = makeContext();
        if (!ctx) return;
        source = ctx.createMediaElementSource(audio);
        // From now on the element only sounds through the graph: keep the DJ hearing it.
        source.connect(ctx.destination);
      }
      if (!ctx) return;
      void ctx.resume?.().catch(() => undefined);
      tap = tapMusicForVoice(ctx, source);
      if (tap) publisher.publishMusic(tap.stream);
    },
    stop,
    dispose() {
      stop();
      void ctx?.close?.().catch(() => undefined);
      ctx = null;
      source = null;
    },
  };
}

// One jukebox controller per mounted strip, bound to the strip's `<audio>` element.
import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';
import { httpJukeboxApi } from './api';
import { JukeboxController, type JukeboxSnapshot } from './controller';
import { browserMusicSink } from './musicSink';
import type { JukeboxApi } from './types';

export interface UseJukebox {
  snapshot: JukeboxSnapshot;
  controller: JukeboxController | null;
  /** Attach to the `<audio>` element the strip renders. */
  audioRef: (el: HTMLAudioElement | null) => void;
}

const OFF: JukeboxSnapshot = { state: null, status: 'off', driftMs: null };
const NOOP = () => () => undefined;

export function useJukebox(api: JukeboxApi = httpJukeboxApi): UseJukebox {
  const [controller, setController] = useState<JukeboxController | null>(null);
  const el = useRef<HTMLAudioElement | null>(null);
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    const audio = el.current;
    if (!mounted || !audio) return;
    const music = browserMusicSink(audio);
    const c = new JukeboxController({ api, audio, music });
    audio.addEventListener('ended', c.onElementEnded);
    setController(c);
    return () => {
      audio.removeEventListener('ended', c.onElementEnded);
      c.dispose();
      music.dispose();
      setController(null);
    };
  }, [api, mounted]);

  const audioRef = useCallback((node: HTMLAudioElement | null) => {
    el.current = node;
    setMounted(node !== null);
  }, []);

  const snapshot = useSyncExternalStore(
    controller ? controller.subscribe : NOOP,
    controller ? controller.getSnapshot : () => OFF,
    () => OFF,
  );
  return { snapshot, controller, audioRef };
}

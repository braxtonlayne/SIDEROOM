/**
 * MUS-PLY-001: the persistent floating player. One host mounted at the root of the router, so its two `<audio>`
 * decks (the gapless pair, MUS-LIB-001) keep playing across the Lounge, Cinema, Library, Settings and Admin. It
 * renders nothing visible until something is queued. Loaded on demand (see `PlayerGate` in router.tsx), and the
 * music library is connected to the player when it first mounts (`musicWiring.ts`).
 */
import { ThemeProvider } from '@sideroom/ui';
import { useEffect, useRef } from 'react';
import { isPlayerShortcut, matchCustomShortcut } from '../lounge/shortcuts';
import { usePrefsStore } from '../settings/user/prefs';
import { useAppliedTheme } from '../settings/user/useAppliedTheme';
import { createPlayerEngine, restoreResume } from './engine';
import { wireMusicLibrary } from './musicWiring';
import './player.css';
import './skins.css';
import { LibrarySkin } from './skins/LibrarySkin';
import { PillSkin } from './skins/PillSkin';
import { RetroSkin } from './skins/RetroSkin';
import * as actions from './store';
import { usePlayerStore } from './store';

const SKINS = { retro: RetroSkin, pill: PillSkin, library: LibrarySkin } as const;

/** Player keys work on every route, so they live here and not in the Lounge. */
function usePlayerKeys() {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (usePlayerStore.getState().queue.length === 0) return;
      const id = matchCustomShortcut(e, usePrefsStore.getState().prefs.keybinds);
      if (!id || !isPlayerShortcut(id)) return;
      e.preventDefault();
      if (id === 'playerToggle') actions.toggle();
      else if (id === 'playerPrev') actions.prev();
      else if (id === 'playerNext') actions.next();
      else actions.toggleMute();
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, []);
}

/**
 * The dock is fixed and sits outside every section's shell, and each shell (Lounge, Cinema, Library, Admin)
 * defines the `--ui-*` tokens on its own wrapper, not on the page. So the dock brings its own theme root, the
 * member's Lounge theme and appearance settings, instead of borrowing a shell's variables.
 */
function ThemedDock({ children }: { children: React.ReactNode }) {
  const theme = useAppliedTheme(undefined, 'lounge');
  return (
    <ThemeProvider
      section="lounge"
      perSection={theme.perSection}
      appearance={theme.appearance}
      style={{ background: 'transparent', display: 'contents' }}
      {...theme.data}
      className="sr-ply-theme"
    >
      {children}
    </ThemeProvider>
  );
}

export default function PlayerHost() {
  const audioRef = useRef<HTMLAudioElement>(null);
  const nextRef = useRef<HTMLAudioElement>(null);
  const skin = usePlayerStore((s) => s.skin);
  const dock = usePlayerStore((s) => s.dock);
  const queued = usePlayerStore((s) => s.queue.length > 0);

  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;
    wireMusicLibrary();
    restoreResume();
    const engine = createPlayerEngine(audio, nextRef.current ? { nextAudio: nextRef.current } : {});
    // Browser tests (and only they) drive the player through this handle; `audio` is the deck playing now.
    const dev = import.meta.env.DEV || import.meta.env.MODE === 'test';
    if (dev)
      (window as unknown as { __srPlayer?: unknown }).__srPlayer = {
        ...actions,
        get audio() {
          return engine.activeAudio();
        },
        gapless: () => engine.gapless(),
      };
    return () => {
      engine.destroy();
      if (dev) delete (window as unknown as { __srPlayer?: unknown }).__srPlayer;
    };
  }, []);
  usePlayerKeys();

  const Skin = SKINS[skin];
  return (
    <>
      <audio ref={audioRef} data-testid="player-audio" preload="auto" />
      <audio ref={nextRef} data-testid="player-audio-next" preload="auto" />
      {queued && (
        <ThemedDock>
          <div className="sr-ply-dock" data-dock={dock} data-skin={skin}>
            <Skin />
          </div>
        </ThemedDock>
      )}
    </>
  );
}

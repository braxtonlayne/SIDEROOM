import { ToastProvider } from '@sideroom/ui';
import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';
import { EmulatorSwitchedOff, useEmulatorOn } from './SwitchedOff';

// The Arcade and the emulator are chunks of the signed-in app only: nothing here loads before sign-in (D-385).
const EmuPage = lazyNamed(() => import('./EmuPage'), 'EmuPage');
const ArcadePage = lazyNamed(() => import('../arcade/ArcadePage'), 'ArcadePage');
// The Arcade's own themed section (D-476, editorial by default); a chunk of its own, so the first paint does not grow.
const ArcadeThemeProvider = lazyNamed(() => import('../arcade/ArcadeTheme'), 'ArcadeThemeProvider');
// D-480: the Emulator is a room of the Arcade, so it carries the Arcade's rooms bar (its own small chunk).
const ArcadeRoomBar = lazyNamed(() => import('../arcade/ArcadeShell'), 'ArcadeRoomBar');

/** /games: the Arcade (ARC-ROOM-001); /games/emulator: the emulator (ACT-EMU-001). */
export function createEmuRoutes(root: AnyRoute) {
  const arcade = createRoute({
    getParentRoute: () => root,
    path: '/games',
    component: function GamesRoute() {
      return (
        <ArcadeThemeProvider>
          <ToastProvider>
            <ArcadePage />
          </ToastProvider>
        </ArcadeThemeProvider>
      );
    },
  });
  const emulator = createRoute({
    getParentRoute: () => root,
    path: '/games/emulator',
    component: function EmulatorRoute() {
      const on = useEmulatorOn();
      return (
        <ArcadeThemeProvider>
          <ToastProvider>
            <ArcadeRoomBar />
            {on ? <EmuPage /> : <EmulatorSwitchedOff />}
          </ToastProvider>
        </ArcadeThemeProvider>
      );
    },
  });
  return [arcade, emulator];
}

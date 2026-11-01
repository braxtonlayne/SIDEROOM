import { ThemeProvider, ToastProvider } from '@sideroom/ui';
import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';
import { EmulatorSwitchedOff, useEmulatorOn } from './SwitchedOff';

// The Arcade and the emulator are chunks of the signed-in app only: nothing here loads before sign-in (D-385).
const EmuPage = lazyNamed(() => import('./EmuPage'), 'EmuPage');
const ArcadePage = lazyNamed(() => import('../arcade/ArcadePage'), 'ArcadePage');

/** /games: the Arcade (ARC-ROOM-001); /games/emulator: the emulator (ACT-EMU-001). */
export function createEmuRoutes(root: AnyRoute) {
  const arcade = createRoute({
    getParentRoute: () => root,
    path: '/games',
    component: function GamesRoute() {
      return (
        <ThemeProvider section="lounge">
          <ToastProvider>
            <ArcadePage />
          </ToastProvider>
        </ThemeProvider>
      );
    },
  });
  const emulator = createRoute({
    getParentRoute: () => root,
    path: '/games/emulator',
    component: function EmulatorRoute() {
      const on = useEmulatorOn();
      return (
        <ThemeProvider section="lounge">
          <ToastProvider>{on ? <EmuPage /> : <EmulatorSwitchedOff />}</ToastProvider>
        </ThemeProvider>
      );
    },
  });
  return [arcade, emulator];
}

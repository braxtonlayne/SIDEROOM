import { ThemeProvider, ToastProvider } from '@sideroom/ui';
import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';
import { EmulatorSwitchedOff, useEmulatorOn } from './SwitchedOff';

// The emulator is a chunk of the signed-in app only: nothing here loads before sign-in (D-385).
const EmuPage = lazyNamed(() => import('./EmuPage'), 'EmuPage');

/** /games: the emulator (ACT-EMU-001). */
export function createEmuRoutes(root: AnyRoute) {
  const route = createRoute({
    getParentRoute: () => root,
    path: '/games',
    component: function GamesRoute() {
      const on = useEmulatorOn();
      return (
        <ThemeProvider section="lounge">
          <ToastProvider>{on ? <EmuPage /> : <EmulatorSwitchedOff />}</ToastProvider>
        </ThemeProvider>
      );
    },
  });
  return [route];
}

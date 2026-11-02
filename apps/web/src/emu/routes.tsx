import { ToastProvider } from '@sideroom/ui';
import { createRoute, type AnyRoute } from '@tanstack/react-router';
import { lazyNamed } from '../lib/lazyRoute';
import { EmulatorSwitchedOff, useEmulatorOn } from './SwitchedOff';

// The Arcade and the emulator are chunks of the signed-in app only: nothing here loads before sign-in (D-385).
const EmuPage = lazyNamed(() => import('./EmuPage'), 'EmuPage');
const ArcadePage = lazyNamed(() => import('../arcade/ArcadePage'), 'ArcadePage');
// The viewer's saved theme, like the Lounge (XC-VIS-001); a chunk of its own, so the first paint does not grow.
const LoungeThemeProvider = lazyNamed(
  () => import('../settings/user/LoungeThemeProvider'),
  'LoungeThemeProvider',
);

/** /games: the Arcade (ARC-ROOM-001); /games/emulator: the emulator (ACT-EMU-001). */
export function createEmuRoutes(root: AnyRoute) {
  const arcade = createRoute({
    getParentRoute: () => root,
    path: '/games',
    component: function GamesRoute() {
      return (
        <LoungeThemeProvider>
          <ToastProvider>
            <ArcadePage />
          </ToastProvider>
        </LoungeThemeProvider>
      );
    },
  });
  const emulator = createRoute({
    getParentRoute: () => root,
    path: '/games/emulator',
    component: function EmulatorRoute() {
      const on = useEmulatorOn();
      return (
        <LoungeThemeProvider>
          <ToastProvider>{on ? <EmuPage /> : <EmulatorSwitchedOff />}</ToastProvider>
        </LoungeThemeProvider>
      );
    },
  });
  return [arcade, emulator];
}

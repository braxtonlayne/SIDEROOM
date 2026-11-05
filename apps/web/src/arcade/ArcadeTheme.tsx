import { ThemeProvider } from '@sideroom/ui';
import type { ReactNode } from 'react';
import { useAppliedTheme } from '../settings/user/useAppliedTheme';

/**
 * The Arcade's theme (D-476): an editorial room like the Cinema and the Library. It resolves the `arcade` section the
 * way `CinemaRoot` resolves `cinema`: the member's own choice for the Arcade (Settings > Appearance > Sections), then
 * the Operator's platform default, then the shipped `lounge-editorial`. The member's Lounge theme does not carry over.
 */
export function ArcadeThemeProvider({ children }: { children: ReactNode }) {
  const theme = useAppliedTheme(undefined, 'arcade');
  return (
    <ThemeProvider
      section="arcade"
      perSection={theme.perSection}
      appearance={theme.appearance}
      style={{ minBlockSize: '100dvh', ...theme.style }}
      {...theme.data}
    >
      {children}
    </ThemeProvider>
  );
}

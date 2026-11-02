import { ThemeProvider } from '@sideroom/ui';
import type { CSSProperties, ReactNode } from 'react';
import { useAppliedTheme } from './useAppliedTheme';

/**
 * A full page in the Lounge's look that follows the viewer's saved theme (XC-VIS-001). Server Settings, the Arcade,
 * the emulator and the not-found page used a bare `<ThemeProvider section="lounge">`, so they stayed on the default
 * dark theme while the Lounge around them was light. The page fills the viewport, so its background does not stop
 * where its content ends.
 */
export function LoungeThemeProvider({
  className,
  children,
}: {
  className?: string;
  children: ReactNode;
}) {
  const theme = useAppliedTheme();
  const style: CSSProperties = { minBlockSize: '100dvh', ...theme.style };
  return (
    <ThemeProvider
      section="lounge"
      userGlobal={theme.userGlobal}
      perSection={theme.perSection}
      allowServerTheming={theme.allowServerTheming}
      appearance={theme.appearance}
      style={style}
      {...theme.data}
      className={className}
    >
      {children}
    </ThemeProvider>
  );
}

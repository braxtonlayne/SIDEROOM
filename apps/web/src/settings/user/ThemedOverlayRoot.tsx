import { ThemeProvider, useTheme, zoomVars } from '@sideroom/ui';
import { useMemo, type CSSProperties, type ReactNode } from 'react';
import { useAppliedTheme } from './useAppliedTheme';
import { OVERLAY_ZOOM_VAR } from './overlayZoom';
import './overlay-root.css';

/**
 * Renders the overlays only once the theme root exists. `ThemeProvider` learns its own element through a ref, so on
 * the first render its portal root is still null and the kit's `Modal` falls back to `document.body`; one commit later
 * the same modal is re-portalled into the root, which remounts its content. The drawer opens on that first render
 * (Mod+K mounts the host), so its search field was focused, then replaced, and keys typed next went nowhere (the
 * "crash" of the first attempt: the page waited for a tool that never opened). One empty frame instead.
 */
function WhenRooted({ children }: { children: ReactNode }) {
  const { portalRoot } = useTheme();
  return portalRoot ? <>{children}</> : null;
}

/**
 * The theme root for the app's route-outliving overlays: the player dock (MUS-PLY-001) and the utility drawer with
 * its gadgets and pin rail (UTL-SHL-001, D-538). They are fixed, sit outside every section's shell, and each shell
 * defines the `--ui-*` tokens on its own wrapper, so they bring their own: the member's Lounge theme (Light, Dark,
 * Editorial or a Theme Store theme) and appearance settings, in every section, so nothing re-skins at a section
 * boundary. The root is `display: contents`: it adds no box and no stacking context, so the overlays keep their own
 * z-index order against the page (gadgets 909, player 910, modals 1000). The member's zoom (D-540) cannot go on a
 * box-less root, so the root carries it as `--sr-overlay-zoom` and `overlay-root.css` applies it to each child.
 */
export function ThemedOverlayRoot({
  className,
  children,
}: {
  className: string;
  children: ReactNode;
}) {
  const theme = useAppliedTheme(undefined, 'lounge');
  const style = useMemo(() => {
    // A real `zoom` box here would restack the overlays (D-540): the children take it from the custom property.
    const rest: Record<string, unknown> = { ...theme.style };
    const zoom = typeof rest.zoom === 'number' && rest.zoom > 0 ? rest.zoom : 1;
    delete rest.zoom;
    return {
      ...rest,
      [OVERLAY_ZOOM_VAR]: String(zoom),
      // The kit's own token (D-555): its dialogs and menus in the zoomed children divide viewport maxima by it.
      ...zoomVars(zoom),
      background: 'transparent',
      display: 'contents',
    } as CSSProperties;
  }, [theme.style]);
  return (
    <ThemeProvider
      section="lounge"
      userGlobal={theme.userGlobal}
      perSection={theme.perSection}
      appearance={theme.appearance}
      style={style}
      {...theme.data}
      className={`sr-overlay-root ${className}`}
    >
      <WhenRooted>{children}</WhenRooted>
    </ThemeProvider>
  );
}

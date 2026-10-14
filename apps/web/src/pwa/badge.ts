/**
 * Badging API wrapper (XC-PWA-005). The Lounge unread store calls `setAppBadge(total)`; browsers without
 * the API, and tabs that are not installed apps, ignore it silently.
 */
type BadgeNavigator = Navigator & {
  setAppBadge?: (n?: number) => Promise<void>;
  clearAppBadge?: () => Promise<void>;
};

export function setAppBadge(count: number): void {
  const nav = navigator as BadgeNavigator;
  try {
    if (count > 0) void nav.setAppBadge?.(count)?.catch(() => undefined);
    else void nav.clearAppBadge?.()?.catch(() => undefined);
  } catch {
    /* not supported */
  }
}

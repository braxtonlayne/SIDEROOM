// What a server's managers chose for its look (LNG-SSET-050): a server-wide theme and an accent colour. Members'
// clients read it when they open the server and again on `SR_GUILD_APPEARANCE_UPDATE`. A theme is only design
// tokens (D-063); they are checked again here before they touch the DOM, like a Theme Store theme.
//
// Precedence (D-280), from the bottom: the built-in theme, the platform default, the member's own theme (global or
// per section), the server theme (Lounge of that server only, D-281), the server accent, a Theme Store preview.
// One switch, "Allow server themes", covers theme and accent together. The manager's accent wins over the theme's
// accent token because it is the narrower, later choice; the token is only the author's default.
import { useEffect, useMemo, type CSSProperties } from 'react';
import { serverAccentTokens } from '@sideroom/ui';
import { create } from 'zustand';
import { onDispatch } from '../store/events';
import { guildAppearance, type GuildAppearance } from './api';
import { safeTokens } from './themes';

interface State {
  byGuild: Record<string, GuildAppearance>;
  load: (guildId: string) => Promise<void>;
}

export const useGuildAppearances = create<State>()((set) => ({
  byGuild: {},
  load: async (guildId) => {
    try {
      const a = await guildAppearance(guildId);
      set((s) => ({ byGuild: { ...s.byGuild, [guildId]: a } }));
    } catch {
      /* offline or not a member: keep what we had */
    }
  },
}));

/** `#rrggbb` of a 0xRRGGBB number, or null. */
export const hexOfAccent = (n: number | null | undefined): string | null =>
  typeof n === 'number' && Number.isInteger(n) && n >= 0 && n <= 0xffffff
    ? `#${n.toString(16).padStart(6, '0')}`
    : null;

/** 0xRRGGBB of `#rrggbb`, or null when it is not one. */
export const accentOfHex = (hex: string): number | null =>
  /^#[0-9a-fA-F]{6}$/.test(hex) ? parseInt(hex.slice(1), 16) : null;

/**
 * The server's accent colour and theme tokens for the server in view. Nothing applies when the viewer turned
 * server theming off (Appearance settings) or there is no server (Home).
 */
export function useServerAppearance(
  guildId: string | undefined,
  allow: boolean,
): { accent: string | null; style: CSSProperties } {
  const load = useGuildAppearances((s) => s.load);
  const a = useGuildAppearances((s) => (guildId ? s.byGuild[guildId] : undefined));
  useEffect(() => {
    if (!guildId) return;
    void load(guildId);
    return onDispatch((event, data) => {
      if (
        event === 'SR_GUILD_APPEARANCE_UPDATE' &&
        (data as { guild_id?: string })?.guild_id === guildId
      )
        void load(guildId);
    });
  }, [guildId, load]);
  return useMemo(() => {
    if (!allow || !a) return { accent: null, style: {} };
    const accent = hexOfAccent(a.accent_color);
    const theme = a.theme;
    const style: CSSProperties = theme
      ? ({ ...safeTokens(theme.tokens), colorScheme: theme.scheme } as CSSProperties)
      : {};
    // The manager's accent colour wins over whatever accent the theme names.
    return { accent, style: { ...style, ...(accent ? serverAccentTokens(accent) : {}) } };
  }, [a, allow]);
}

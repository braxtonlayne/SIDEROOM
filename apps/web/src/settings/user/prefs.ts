/**
 * User preferences (appearance, accessibility, text and images, notifications, activity privacy, keybinds,
 * language, advanced). The browser copy (localStorage `sr.user.prefs`) is the first paint and the offline
 * cache; the account copy lives in the settings blob (`GET/PATCH /users/@me/settings`, key `user_prefs`) and
 * is kept in step by `prefsSync.ts`, so the choices follow the user to every browser.
 *
 * Every field is validated on the way in (`sanitizePrefs`), so stale or hand-edited storage and a blob written
 * by a newer client can never break rendering. Applied through the ThemeProvider runtime variables
 * (docs/10 section 6) and `data-*` attributes (see `prefs-effects.css`).
 */
import type { AppearanceSettings, ThemeId } from '@sideroom/ui';
import { isThemeId } from '@sideroom/ui';
import type { CSSProperties } from 'react';
import { create } from 'zustand';
import { getHour12, setFormatLocale, setHour12 } from '../../i18n';
import { createJSONStorage, persist, type StateStorage } from 'zustand/middleware';
import './prefs-effects.css';
import { withPlatformDefaults, type PlatformThemes } from './platformThemes';

export const THEME_CHOICES = [
  'lounge-dark',
  'lounge-light',
  'lounge-ash',
  'lounge-onyx',
  'sideroom',
] as const;
export type ThemeChoice = 'system' | (typeof THEME_CHOICES)[number];
export type MessageDisplay = 'cozy' | 'compact';

/** Sections with their own theme picker (Settings > Appearance). */
export const THEMED_SECTIONS = ['lounge', 'cinema', 'library', 'entrance'] as const;
export type ThemedSection = (typeof THEMED_SECTIONS)[number];
export type SectionThemes = Partial<Record<ThemedSection, ThemeId>>;

export type RoleColors = 'name' | 'dot' | 'off';
export type Animation = 'always' | 'hover' | 'never';
export type SpoilerMode = 'click' | 'mine' | 'always';
export type TimeFormat = 'auto' | '12h' | '24h';
export type MediaSize = 'small' | 'medium' | 'large';
/** Top-tab mode of Cinema and Library (BYOS-UI-001): always show, always hide, or hide when nothing is there. */
export type NavMode = 'show' | 'hide' | 'auto';

export interface UserPrefs {
  // Appearance
  theme: ThemeChoice;
  /** Per-section overrides; a missing section follows the theme above (Lounge) or its own default. */
  sectionThemes: SectionThemes;
  /** Cinema and Library top tabs (Appearance > Sections). */
  navCinema: NavMode;
  navLibrary: NavMode;
  messageDisplay: MessageDisplay;
  /** A small avatar before the name on every line in Compact mode (LNG-USET-040). */
  compactAvatars: boolean;
  /** Pixels between message groups, 0..24. */
  messageGap: number;
  /** Percent of the 16px base font, 75..150. */
  fontScale: number;
  /** 0..100. */
  saturation: number;
  /** Interface zoom percent, 50..200. */
  zoom: number;
  /** Let a server recolour the interface with its accent colour. */
  allowServerTheming: boolean;
  // Accessibility
  reducedMotion: AppearanceSettings['reducedMotion'];
  highContrast: boolean;
  underlineLinks: boolean;
  roleColors: RoleColors;
  dyslexiaFont: boolean;
  stickerAnimation: Animation;
  emojiAnimation: Animation;
  /** Play animated GIFs while the client has focus; off (or reduced motion) shows the first frame until hover or focus (RDY-PAR-006). */
  gifAutoplay: boolean;
  showSendButton: boolean;
  /** Text-to-speech speed for "Speak message", percent of normal, 50..200 (LNG-USET-042). */
  ttsRate: number;
  // Text and images
  linkPreviews: boolean;
  showEmbeds: boolean;
  inlineAttachments: boolean;
  showReactions: boolean;
  convertEmoticons: boolean;
  /** 'mine' reveals a spoiler in your own messages while the pointer is over it (LNG-USET-051). */
  spoilers: SpoilerMode;
  /** Offer matching stickers while the box holds just a word or two (LNG-USET-051). */
  stickerSuggestions: boolean;
  /** Show the rendered Markdown under the message box while typing (LNG-USET-051). */
  markdownPreview: boolean;
  /** Largest an attached image or video is drawn inline in a message (LNG-USET-051). */
  inlineMediaSize: MediaSize;
  // Notifications (the default level itself is stored on the server: /users/@me/notification-defaults)
  desktopNotifications: boolean;
  soundMessage: boolean;
  soundMention: boolean;
  soundCall: boolean;
  unreadBadge: boolean;
  titleBlink: boolean;
  reactionNotifications: boolean;
  /** A toast inside the app for a direct message that arrives while another channel is open (LNG-NTF-009). */
  dmToasts: boolean;
  /** Minutes without activity before notifications go to the phone instead (0 = never). */
  pushInactiveMinutes: number;
  emailDigest: boolean;
  // Activity privacy
  shareActivity: boolean;
  shareWatching: boolean;
  shareReading: boolean;
  hideAgeRestrictedActivity: boolean;
  pauseHistory: boolean;
  activityAsStatus: boolean;
  shareDetectedActivities: boolean;
  /** The first-run welcome was shown and closed (ENT-UI-022). */
  welcomeDone: boolean;
  // Language
  locale: string;
  timeFormat: TimeFormat;
  // Keybinds: shortcut id -> combo ("Mod+Shift+K"); '' = unbound; absent = the default.
  keybinds: Record<string, string>;
  // Streamer Mode (LNG-USET-055)
  streamerMode: boolean;
  streamerHidePersonal: boolean;
  streamerHideInvites: boolean;
  streamerSilence: boolean;
  // Advanced
  developerMode: boolean;
  /** Round camera bubbles over the watch party stage while in a call (D-265). On by default; "Hide cameras" turns it off. */
  watchPartyCameras: boolean;
  /** Shows the diagnostics card and the pseudo-languages for people building on SIDE ROOM. */
  appTestMode: boolean;
}

export const DEFAULT_PREFS: UserPrefs = {
  // The Lounge has always been dark by default; "Sync with computer" is opt-in.
  theme: 'lounge-dark',
  sectionThemes: {},
  navCinema: 'show',
  navLibrary: 'show',
  messageDisplay: 'cozy',
  compactAvatars: false,
  messageGap: 16,
  fontScale: 100,
  saturation: 100,
  zoom: 100,
  allowServerTheming: true,
  reducedMotion: 'system',
  highContrast: false,
  underlineLinks: false,
  roleColors: 'name',
  dyslexiaFont: false,
  stickerAnimation: 'always',
  emojiAnimation: 'always',
  gifAutoplay: true,
  showSendButton: true,
  ttsRate: 100,
  linkPreviews: true,
  showEmbeds: true,
  inlineAttachments: true,
  showReactions: true,
  convertEmoticons: true,
  spoilers: 'click',
  stickerSuggestions: true,
  markdownPreview: false,
  inlineMediaSize: 'medium',
  desktopNotifications: false,
  soundMessage: true,
  soundMention: true,
  soundCall: true,
  unreadBadge: true,
  titleBlink: false,
  reactionNotifications: true,
  dmToasts: true,
  pushInactiveMinutes: 10,
  emailDigest: false,
  shareActivity: true,
  shareWatching: true,
  shareReading: true,
  hideAgeRestrictedActivity: true,
  pauseHistory: false,
  activityAsStatus: false,
  shareDetectedActivities: true,
  welcomeDone: false,
  locale: 'auto',
  timeFormat: 'auto',
  keybinds: {},
  streamerMode: false,
  streamerHidePersonal: true,
  streamerHideInvites: true,
  streamerSilence: true,
  developerMode: false,
  watchPartyCameras: true,
  appTestMode: false,
};

export const TTS_RATE_RANGE = [50, 200] as const;
export const FONT_SCALE_RANGE = [75, 150] as const;
export const ZOOM_RANGE = [50, 200] as const;
export const MESSAGE_GAP_RANGE = [0, 24] as const;
export const PUSH_INACTIVE_CHOICES = [1, 5, 10, 15, 30, 60, 0] as const;

const clamp = (n: unknown, lo: number, hi: number, fallback: number) =>
  typeof n === 'number' && Number.isFinite(n)
    ? Math.min(hi, Math.max(lo, Math.round(n)))
    : fallback;
const oneOf = <T extends string>(v: unknown, allowed: readonly T[], fallback: T): T =>
  typeof v === 'string' && (allowed as readonly string[]).includes(v) ? (v as T) : fallback;
const bool = (v: unknown, fallback: boolean) => (typeof v === 'boolean' ? v : fallback);
const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === 'object' && v !== null && !Array.isArray(v);

/** Combos look like "Mod+Shift+K": modifiers first, then one key. */
const COMBO = /^(?:(?:Mod|Alt|Shift)\+){0,3}[^+\s]+$|^(?:(?:Mod|Alt|Shift)\+){0,3}\+$/;
export const isCombo = (v: unknown): v is string => typeof v === 'string' && COMBO.test(v);
const LOCALE = /^(auto|[a-z]{2,3}(-[A-Za-z0-9]{2,8})*)$/;

/** Stale or hand-edited storage can never break rendering: every field is validated. */
export function sanitizePrefs(raw: unknown): UserPrefs {
  const r = isRecord(raw) ? raw : {};
  const d = DEFAULT_PREFS;
  const sectionThemes: SectionThemes = {};
  if (isRecord(r.sectionThemes))
    for (const s of THEMED_SECTIONS) {
      const v = r.sectionThemes[s];
      if (isThemeId(v)) sectionThemes[s] = v;
    }
  const keybinds: Record<string, string> = {};
  if (isRecord(r.keybinds))
    for (const k of Object.keys(r.keybinds).sort()) {
      const v = r.keybinds[k];
      if (v === '' || isCombo(v)) keybinds[k] = v;
    }
  const anim = ['always', 'hover', 'never'] as const;
  return {
    theme: oneOf(r.theme, ['system', ...THEME_CHOICES] as const, d.theme),
    sectionThemes,
    navCinema: oneOf(r.navCinema, ['show', 'hide', 'auto'] as const, d.navCinema),
    navLibrary: oneOf(r.navLibrary, ['show', 'hide', 'auto'] as const, d.navLibrary),
    messageDisplay: oneOf(r.messageDisplay, ['cozy', 'compact'] as const, d.messageDisplay),
    compactAvatars: bool(r.compactAvatars, d.compactAvatars),
    messageGap: clamp(r.messageGap, ...MESSAGE_GAP_RANGE, d.messageGap),
    fontScale: clamp(r.fontScale, ...FONT_SCALE_RANGE, d.fontScale),
    saturation: clamp(r.saturation, 0, 100, d.saturation),
    zoom: clamp(r.zoom, ...ZOOM_RANGE, d.zoom),
    allowServerTheming: bool(r.allowServerTheming, d.allowServerTheming),
    reducedMotion: oneOf(r.reducedMotion, ['system', 'on', 'off'] as const, d.reducedMotion),
    highContrast: bool(r.highContrast, d.highContrast),
    underlineLinks: bool(r.underlineLinks, d.underlineLinks),
    roleColors: oneOf(r.roleColors, ['name', 'dot', 'off'] as const, d.roleColors),
    dyslexiaFont: bool(r.dyslexiaFont, d.dyslexiaFont),
    stickerAnimation: oneOf(r.stickerAnimation, anim, d.stickerAnimation),
    emojiAnimation: oneOf(r.emojiAnimation, anim, d.emojiAnimation),
    gifAutoplay: bool(r.gifAutoplay, d.gifAutoplay),
    showSendButton: bool(r.showSendButton, d.showSendButton),
    ttsRate: clamp(r.ttsRate, TTS_RATE_RANGE[0], TTS_RATE_RANGE[1], d.ttsRate),
    linkPreviews: bool(r.linkPreviews, d.linkPreviews),
    showEmbeds: bool(r.showEmbeds, d.showEmbeds),
    inlineAttachments: bool(r.inlineAttachments, d.inlineAttachments),
    showReactions: bool(r.showReactions, d.showReactions),
    convertEmoticons: bool(r.convertEmoticons, d.convertEmoticons),
    spoilers: oneOf(r.spoilers, ['click', 'mine', 'always'] as const, d.spoilers),
    stickerSuggestions: bool(r.stickerSuggestions, d.stickerSuggestions),
    markdownPreview: bool(r.markdownPreview, d.markdownPreview),
    inlineMediaSize: oneOf(
      r.inlineMediaSize,
      ['small', 'medium', 'large'] as const,
      d.inlineMediaSize,
    ),
    desktopNotifications: bool(r.desktopNotifications, d.desktopNotifications),
    soundMessage: bool(r.soundMessage, d.soundMessage),
    soundMention: bool(r.soundMention, d.soundMention),
    soundCall: bool(r.soundCall, d.soundCall),
    unreadBadge: bool(r.unreadBadge, d.unreadBadge),
    titleBlink: bool(r.titleBlink, d.titleBlink),
    reactionNotifications: bool(r.reactionNotifications, d.reactionNotifications),
    dmToasts: bool(r.dmToasts, d.dmToasts),
    pushInactiveMinutes: (PUSH_INACTIVE_CHOICES as readonly number[]).includes(
      r.pushInactiveMinutes as number,
    )
      ? (r.pushInactiveMinutes as number)
      : d.pushInactiveMinutes,
    emailDigest: bool(r.emailDigest, d.emailDigest),
    shareActivity: bool(r.shareActivity, d.shareActivity),
    shareWatching: bool(r.shareWatching, d.shareWatching),
    shareReading: bool(r.shareReading, d.shareReading),
    hideAgeRestrictedActivity: bool(r.hideAgeRestrictedActivity, d.hideAgeRestrictedActivity),
    pauseHistory: bool(r.pauseHistory, d.pauseHistory),
    activityAsStatus: bool(r.activityAsStatus, d.activityAsStatus),
    shareDetectedActivities: bool(r.shareDetectedActivities, d.shareDetectedActivities),
    welcomeDone: bool(r.welcomeDone, d.welcomeDone),
    locale: typeof r.locale === 'string' && LOCALE.test(r.locale) ? r.locale : d.locale,
    timeFormat: oneOf(r.timeFormat, ['auto', '12h', '24h'] as const, d.timeFormat),
    keybinds,
    streamerMode: bool(r.streamerMode, d.streamerMode),
    streamerHidePersonal: bool(r.streamerHidePersonal, d.streamerHidePersonal),
    streamerHideInvites: bool(r.streamerHideInvites, d.streamerHideInvites),
    streamerSilence: bool(r.streamerSilence, d.streamerSilence),
    developerMode: bool(r.developerMode, d.developerMode),
    watchPartyCameras: bool(r.watchPartyCameras, d.watchPartyCameras),
    appTestMode: bool(r.appTestMode, d.appTestMode),
  };
}

export const prefsEqual = (a: UserPrefs, b: UserPrefs) => JSON.stringify(a) === JSON.stringify(b);

const safeStorage: StateStorage = {
  getItem: (k) => {
    try {
      return localStorage.getItem(k);
    } catch {
      return null;
    }
  },
  setItem: (k, v) => {
    try {
      localStorage.setItem(k, v);
    } catch {
      /* storage unavailable: preferences simply are not persisted */
    }
  },
  removeItem: (k) => {
    try {
      localStorage.removeItem(k);
    } catch {
      /* ignore */
    }
  },
};

interface PrefsState {
  prefs: UserPrefs;
  /** The user saved these (Settings > Save): `prefsSync` also pushes them to the account. */
  save(next: UserPrefs): void;
  /** The account copy arrived (or changed on another device): adopt it without writing it back. */
  adopt(next: UserPrefs): void;
}

export const usePrefsStore = create<PrefsState>()(
  persist(
    (set) => ({
      prefs: DEFAULT_PREFS,
      save: (next) => set({ prefs: sanitizePrefs(next) }),
      adopt: (next) => set({ prefs: sanitizePrefs(next) }),
    }),
    {
      name: 'sr.user.prefs',
      version: 2,
      storage: createJSONStorage(() => safeStorage),
      partialize: (s) => ({ prefs: s.prefs }),
      merge: (persisted, current) => ({
        ...current,
        prefs: sanitizePrefs((persisted as { prefs?: unknown } | undefined)?.prefs),
      }),
    },
  ),
);

/** Dates, numbers and the 12/24 hour clock everywhere follow the Language prefs (XC-I18N-002). */
function syncFormatting(p: UserPrefs) {
  const hour12 = p.timeFormat === 'auto' ? undefined : p.timeFormat === '12h';
  if (getHour12() !== hour12) setHour12(hour12);
  setFormatLocale(p.locale === 'auto' ? undefined : p.locale);
}
syncFormatting(usePrefsStore.getState().prefs);
usePrefsStore.subscribe((s) => syncFormatting(s.prefs));

export const useSavedPrefs = () => usePrefsStore((s) => s.prefs);

/** Developer Mode (G-56, D-195): the "Copy ID" menu entries render only while this is on. */
export const useDeveloperMode = () => usePrefsStore((s) => s.prefs.developerMode);

/** Streamer Mode switches in effect right now (saved prefs; an unsaved toggle does not count). */
export function streamerState(p: UserPrefs = usePrefsStore.getState().prefs) {
  return {
    personal: p.streamerMode && p.streamerHidePersonal,
    invites: p.streamerMode && p.streamerHideInvites,
    silence: p.streamerMode && p.streamerSilence,
  };
}
export const useStreamer = () => streamerState(usePrefsStore((s) => s.prefs));

export interface AppliedTheme {
  userGlobal: ThemeId;
  perSection: PlatformThemes;
  allowServerTheming: boolean;
  appearance: Partial<AppearanceSettings>;
  style: CSSProperties;
  messageDisplay: MessageDisplay;
  /** `data-*` attributes for the ThemeProvider root (CSS in `prefs-effects.css`). */
  data: Record<`data-${string}`, string | undefined>;
}

/** What to hand a ThemeProvider for a given set of prefs. `systemDark` resolves "Sync with computer". */
export function applyPrefs(
  p: UserPrefs,
  systemDark: boolean,
  platform: PlatformThemes = {},
): AppliedTheme {
  const userGlobal: ThemeId =
    p.theme === 'system' ? (systemDark ? 'lounge-dark' : 'lounge-light') : p.theme;
  const style: Record<string, string | number> = {};
  // CSS zoom scales layout and text together; it is a no-op at 100.
  if (p.zoom !== 100) style.zoom = p.zoom / 100;
  style['--lng-message-gap'] = `${p.messageGap}px`;
  return {
    userGlobal,
    // A theme chosen for everything is the viewer's own choice; the unchanged default is not.
    perSection: withPlatformDefaults(p.sectionThemes, p.theme !== DEFAULT_PREFS.theme, platform),
    allowServerTheming: p.allowServerTheming,
    appearance: {
      fontSize: (16 * p.fontScale) / 100,
      saturation: p.saturation,
      reducedMotion: p.reducedMotion,
      contrast: p.highContrast ? 'high' : 'normal',
    },
    style: style as CSSProperties,
    messageDisplay: p.messageDisplay,
    data: {
      'data-message-display': p.messageDisplay,
      'data-underline-links': p.underlineLinks ? 'true' : undefined,
      'data-dyslexia': p.dyslexiaFont ? 'true' : undefined,
      'data-media-size': p.inlineMediaSize === 'medium' ? undefined : p.inlineMediaSize,
      'data-streamer-personal': streamerState(p).personal ? 'true' : undefined,
    },
  };
}

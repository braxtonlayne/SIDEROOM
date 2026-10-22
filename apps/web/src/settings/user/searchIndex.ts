import { USER_SETTINGS_SECTIONS, type UserSettingsSectionId } from './nav';

/** One searchable setting: the section it lives in, the card to scroll to, and extra words people may type. */
export interface SettingsEntry {
  section: UserSettingsSectionId;
  /** Element id of the card (see `Card id=...`). */
  card?: string;
  label: string;
  keywords?: string;
}

export const SETTINGS_INDEX: readonly SettingsEntry[] = [
  { section: 'account', card: 'us-identity', label: 'Username and email', keywords: 'name login' },
  { section: 'account', card: 'us-password', label: 'Change password', keywords: 'security' },
  {
    section: 'account',
    card: 'us-2fa',
    label: 'Two-factor authentication',
    keywords: '2fa totp authenticator backup recovery codes',
  },
  {
    section: 'account',
    card: 'us-passkeys',
    label: 'Passkeys and security keys',
    keywords: 'webauthn',
  },
  { section: 'account', card: 'us-removal', label: 'Delete account', keywords: 'account removal' },
  {
    section: 'profile',
    card: 'us-profile',
    label: 'Display name, pronouns, about me',
    keywords: 'bio status accent color',
  },
  {
    section: 'profile',
    card: 'us-staff-badge',
    label: 'Show the SIDE ROOM Staff badge',
    keywords: 'staff badge profile hide',
  },
  {
    section: 'notifications',
    card: 'us-platform-dm',
    label: 'Platform announcements by direct message',
    keywords: 'announcement dm system message sideroom',
  },
  {
    section: 'profile',
    card: 'us-media',
    label: 'Avatar and banner',
    keywords: 'picture photo crop upload',
  },
  {
    section: 'profile',
    card: 'us-server-profiles',
    label: 'Server nickname',
    keywords: 'per-server profile',
  },
  {
    section: 'privacy',
    card: 'us-who',
    label: 'Who can DM you and send friend requests',
    keywords: 'direct messages message requests spam',
  },
  {
    section: 'privacy',
    card: 'us-remote-images',
    label: 'Load remote images',
    keywords: 'embeds ip address',
  },
  { section: 'privacy', card: 'us-blocked', label: 'Blocked users', keywords: 'unblock' },
  { section: 'privacy', card: 'us-ignored', label: 'Ignored users', keywords: 'unignore' },
  {
    section: 'privacy',
    card: 'us-data',
    label: 'Request my data',
    keywords: 'download export gdpr',
  },
  {
    section: 'privacy',
    card: 'us-dm-filter',
    label: 'DM spam filter and server DMs',
    keywords: 'direct messages per server override',
  },
  {
    section: 'privacy',
    card: 'us-age',
    label: 'Age confirmation',
    keywords: '18 adult re-confirm reset',
  },
  {
    section: 'privacy',
    card: 'us-filters',
    label: 'Sensitive media filters',
    keywords: 'nsfw age restricted explicit',
  },
  {
    section: 'activity',
    label: 'Share watching and reading activity',
    keywords: 'status history pause',
  },
  { section: 'apps', label: 'Authorized apps', keywords: 'oauth deauthorize bots' },
  {
    section: 'modules',
    card: 'us-module-community-leveling',
    label: 'Modules',
    keywords: 'personal preferences leveling level-up announcements feature module',
  },
  { section: 'devices', label: 'Devices and sessions', keywords: 'log out sign out trusted' },
  {
    section: 'support',
    label: 'Support / Patron',
    keywords:
      'donate donation patron bitcoin invoice leaderboard invite credits server level boost',
  },
  { section: 'connections', label: 'Connected accounts', keywords: 'discord link' },
  {
    section: 'invites',
    label: 'Invites',
    keywords: 'invite code credits bring friends join link revoke',
  },
  {
    section: 'appearance',
    card: 'us-theme',
    label: 'Theme',
    keywords: 'dark light ash onyx sync computer',
  },
  {
    section: 'appearance',
    card: 'us-section-themes',
    label: 'Per-section themes',
    keywords: 'lounge cinema library entrance',
  },
  {
    section: 'appearance',
    card: 'us-messages',
    label: 'Message display',
    keywords: 'cozy compact spacing',
  },
  {
    section: 'appearance',
    card: 'us-scale',
    label: 'Font scale, zoom and saturation',
    keywords: 'size chat font',
  },
  {
    section: 'appearance',
    card: 'us-server-theming',
    label: 'Allow server theming',
    keywords: 'accent colour',
  },
  { section: 'accessibility', card: 'us-motion', label: 'Reduced motion', keywords: 'animation' },
  { section: 'accessibility', card: 'us-contrast', label: 'High contrast', keywords: 'contrast' },
  {
    section: 'accessibility',
    card: 'us-links',
    label: 'Underline links',
    keywords: 'link decoration',
  },
  { section: 'accessibility', card: 'us-roles', label: 'Role colors', keywords: 'name dot' },
  {
    section: 'accessibility',
    card: 'us-motion',
    label: 'Animated stickers and emoji',
    keywords: 'gif autoplay',
  },
  {
    section: 'accessibility',
    card: 'us-font',
    label: 'Dyslexia-friendly text',
    keywords: 'font spacing send button',
  },
  { section: 'text', card: 'us-previews', label: 'Link previews and embeds', keywords: 'website' },
  {
    section: 'text',
    card: 'us-attachments',
    label: 'Show attachments directly in chat',
    keywords: 'images videos inline',
  },
  {
    section: 'text',
    card: 'us-emoji',
    label: 'Emoji and reactions',
    keywords: 'convert emoticons',
  },
  { section: 'text', card: 'us-spoilers', label: 'Spoilers', keywords: 'reveal' },
  {
    section: 'notifications',
    card: 'us-notify-default',
    label: 'Default notification level',
    keywords: 'mentions all messages',
  },
  {
    section: 'notifications',
    card: 'us-keywords',
    label: 'Notification keywords',
    keywords: 'highlight words',
  },
  {
    section: 'notifications',
    card: 'us-desktop',
    label: 'Desktop notifications',
    keywords: 'badge flash title',
  },
  { section: 'notifications', card: 'us-sounds', label: 'Notification sounds' },
  { section: 'notifications', card: 'us-push', label: 'Push and email', keywords: 'digest' },
  {
    section: 'notifications',
    card: 'us-servers',
    label: 'Server notification overrides',
    keywords: 'mute',
  },
  { section: 'keybinds', label: 'Keybinds', keywords: 'shortcuts hotkeys' },
  { section: 'language', card: 'us-lang', label: 'Language and region', keywords: 'locale' },
  { section: 'language', card: 'us-time', label: '12 or 24-hour clock', keywords: 'time format' },
  {
    section: 'text',
    card: 'us-media-size',
    label: 'Inline media size',
    keywords: 'pictures videos attachments small large',
  },
  {
    section: 'language',
    card: 'us-ui-lang',
    label: 'Interface language',
    keywords: 'translation english',
  },
  {
    section: 'streamer',
    card: 'us-streamer',
    label: 'Streamer Mode',
    keywords: 'hide email invite links obs sounds',
  },
  {
    section: 'advanced',
    card: 'us-test-mode',
    label: 'App test mode',
    keywords: 'diagnostics pseudo language developers',
  },
  { section: 'advanced', card: 'us-dev', label: 'Developer mode', keywords: 'copy id' },
  { section: 'advanced', card: 'us-reset', label: 'Reset UI state', keywords: 'layout panels' },
];

const norm = (s: string) => s.toLowerCase();

export const sectionLabel = (id: UserSettingsSectionId): string =>
  USER_SETTINGS_SECTIONS.find((s) => s.id === id)?.label ?? id;

/** Entries whose label, section name or keywords contain every word of the query. */
export function searchSettings(query: string, limit = 8): SettingsEntry[] {
  const words = norm(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return [];
  return SETTINGS_INDEX.filter((e) => {
    const hay = norm(`${e.label} ${e.keywords ?? ''} ${sectionLabel(e.section)}`);
    return words.every((w) => hay.includes(w));
  }).slice(0, limit);
}

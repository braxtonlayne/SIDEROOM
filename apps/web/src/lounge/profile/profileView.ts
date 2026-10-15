/**
 * One merged, display-ready view of a person's profile (LNG-PRF-003..011): the account profile, overridden by
 * their per-server profile when shown inside a server (LNG-PRF-011), joined with what the store already knows
 * (name, avatar, presence) so the popout can paint before the profile request answers.
 */
import { t } from '../../i18n';
import { cdnUrl } from '../../store/cdn';
import { useGuild, useMembers, usePresence, useUsers, type Presence } from '../data';
import { badgeViews, type BadgeView } from './badgeModel';
import { effectOf, type EffectId } from './decoration';
import { profileTheme, type ProfileTheme } from './theme';
import { accentCss, useProfile, type Profile } from './useProfile';

export interface ProfileView {
  loading: boolean;
  failed: boolean;
  profile: Profile | null;
  name: string;
  handle: string;
  pronouns: string | null;
  bio: string | null;
  avatarUrl: string | null;
  bannerUrl: string | null;
  accent: string | undefined;
  theme: ProfileTheme | null;
  effect: EffectId | null;
  decoration: { asset: string } | null;
  badges: BadgeView[];
  presence: Presence;
  /** The viewer sees this person inside a server and they have a profile of their own there. */
  hasServerProfile: boolean;
  serverTag: string | null;
}

const clean = (s: string | null | undefined) => (s && s.trim() ? s : null);

export function useProfileView(userId: string, guildId?: string): ProfileView {
  const state = useProfile(userId, guildId);
  const users = useUsers();
  const user = users[userId];
  const member = useMembers(guildId).find((m) => m.user.id === userId);
  const guild = useGuild(guildId);
  const presence = usePresence(userId);
  const profile = state.kind === 'ready' ? state.profile : null;
  const gp = profile?.guild_member_profile && guildId ? profile.guild_member_profile : null;
  const up = profile?.user_profile;

  const bannerHash = clean(gp?.banner) ?? clean(up?.banner) ?? clean(profile?.user.banner);
  const avatarHash = clean(gp?.avatar);
  const avatarUrl = avatarHash
    ? cdnUrl('avatars', userId, avatarHash, { size: 128 })
    : profile?.user.avatar
      ? cdnUrl('avatars', userId, profile.user.avatar, { size: 128 })
      : (user?.avatarUrl ?? null);
  const accentNum = gp?.accent_color ?? up?.accent_color;
  const colors = gp?.theme_colors ?? up?.theme_colors;
  const tag = user?.tag ?? clean(profile?.user.primary_guild?.tag);

  return {
    loading: state.kind === 'loading',
    failed: state.kind === 'error',
    profile,
    name:
      clean(gp?.nick) ??
      member?.nick ??
      user?.displayName ??
      profile?.user.global_name ??
      profile?.user.username ??
      t('lounge.profilePopout.unknown'),
    handle: user?.username ?? profile?.user.username ?? '',
    pronouns: clean(gp?.pronouns) ?? clean(up?.pronouns),
    bio: clean(gp?.bio) ?? clean(up?.bio),
    avatarUrl,
    bannerUrl: bannerHash ? cdnUrl('avatars', userId, bannerHash, { size: 600 }) : null,
    accent: accentCss(accentNum),
    theme: profileTheme(colors, accentNum),
    effect: effectOf(gp?.profile_effect ?? up?.profile_effect),
    decoration:
      profile?.user.avatar_decoration_data ??
      (up?.avatar_decoration ? { asset: up.avatar_decoration } : null),
    badges: badgeViews(profile?.badges, {
      isServerOwner: !!guild && guild.ownerId === userId,
      serverTag: tag,
    }),
    presence,
    hasServerProfile: !!gp,
    serverTag: tag ?? null,
  };
}

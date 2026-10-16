import { useViewer } from '../../settings/server/hooks';
import { useChannels } from '../data';

export interface VoicePerms {
  connect: boolean;
  speak: boolean;
  /** Without USE_VAD the channel requires push-to-talk (D-046). */
  useVad: boolean;
  muteMembers: boolean;
  deafenMembers: boolean;
  moveMembers: boolean;
  useSoundboard: boolean;
}

/**
 * The viewer's voice permissions in one channel. Channel overwrites come from the shared Rust engine once it
 * has loaded; until then the guild-level bits apply, and while even those are unknown we assume the
 * permissive default (the server is the security boundary and refuses what is not allowed).
 */
export function useVoicePerms(
  guildId: string | undefined,
  channelId: string | undefined,
): VoicePerms {
  const channels = useChannels(guildId);
  const viewer = useViewer(guildId ?? '');
  const p = channels.find((c) => c.id === channelId)?.permissions;
  const base = (name: string, fallback: boolean) => (viewer.ready ? viewer.can(name) : fallback);
  return {
    connect: p?.connect ?? base('CONNECT', true),
    speak: p?.speak ?? base('SPEAK', true),
    useVad: p?.useVad ?? base('USE_VAD', true),
    muteMembers: p?.muteMembers ?? base('MUTE_MEMBERS', false),
    deafenMembers: p?.deafenMembers ?? base('DEAFEN_MEMBERS', false),
    moveMembers: p?.moveMembers ?? base('MOVE_MEMBERS', false),
    useSoundboard: p?.useSoundboard ?? base('USE_SOUNDBOARD', true),
  };
}

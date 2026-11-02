// ARC-GST-002 (D-397, P28-ARC-1): the reduced `guest` capability set of the Activity SDK bridge, as an ALLOW-list.
// A guest sees and hears only what their own game session produces. A command or event that is not named here does
// not exist for a guest, so a new bridge command added for members later is closed to guests until somebody adds it
// to this file on purpose. (A deny-list would fail open.) `guest.test.ts` pins both lists and walks every command
// the SDK protocol knows.

/** Commands a guest's game may call. Everything else is refused before any code runs. */
export const GUEST_COMMANDS = [
  // The lobby's own roster (nicknames only), the game's data lane and its event subscriptions.
  'getInstanceConnectedParticipants',
  'subscribe',
  'unsubscribe',
  'dataSend',
] as const;
export type GuestCommand = (typeof GUEST_COMMANDS)[number];

/** Events a guest's game may subscribe to. No voice state, no speaking: those belong to the call (D-397). */
// HOST_CHANGED (D-388): which lobby id is the game's host, which the member bridge already tells the lobby.
export const GUEST_EVENTS = [
  'ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE',
  'DATA_MESSAGE',
  'HOST_CHANGED',
] as const;
export type GuestEvent = (typeof GUEST_EVENTS)[number];

/** Manifest capabilities that reach a guest's game (`ready.config.capabilities` is the game's list cut to these). */
export const GUEST_MANIFEST_CAPABILITIES = ['datachannel'] as const;

/** A manifest must declare this to be opened by a guest at all (D-399). */
export const GUEST_PLAY_FLAG = 'guest_play';
/** The lobby sub-room flag; the data lane is the sub-room (D-399). */
export const LOBBY_CHANNEL_FLAG = 'lobby_channel';
/**
 * In-game voice: the validator accepts it and the runner REFUSES it until the capability ships (43-A designs the
 * media leg; there is no voice path in this wave, and a guest must never get the call's audio instead).
 */
export const INGAME_VOICE_FLAG = 'ingame_voice';

export const isGuestCommand = (cmd: string): cmd is GuestCommand =>
  (GUEST_COMMANDS as readonly string[]).includes(cmd);
export const isGuestEvent = (ev: string): ev is GuestEvent =>
  (GUEST_EVENTS as readonly string[]).includes(ev);

/** Why the runner will not start this game for a guest, or `null`. */
export type Refusal = 'ingame_voice' | 'not_guest_play';

export function runnerRefusal(capabilities: readonly string[]): Refusal | null {
  if (capabilities.includes(INGAME_VOICE_FLAG)) return 'ingame_voice';
  if (!capabilities.includes(GUEST_PLAY_FLAG)) return 'not_guest_play';
  return null;
}

/** What the game is told it has: its own declaration cut to the guest allow-list. */
export const guestCapabilities = (declared: readonly string[]): string[] =>
  declared.filter((c) => (GUEST_MANIFEST_CAPABILITIES as readonly string[]).includes(c));

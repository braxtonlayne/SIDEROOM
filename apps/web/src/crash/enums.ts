// The closed enums of the crash ring buffer (OPS-CRASH-001). Kept apart from allowList.ts so the small, eager part of the
// app (the tick ring fed by the Activity bridge) does not pull in the report builder, which loads on the first crash.

/** The ring buffer holds the last 60 ticks. */
export const MAX_TICKS = 60;
export const TICK_SOURCES = ['activity', 'game', 'emulator'] as const;
export type TickSource = (typeof TICK_SOURCES)[number];

/** Opaque tick inputs. An Activity bridge command is named by the command only, never by its arguments. */
export const TICK_INPUTS = [
  'none',
  'up',
  'down',
  'left',
  'right',
  'a',
  'b',
  'x',
  'y',
  'start',
  'select',
  'primary',
  'secondary',
  'jump',
  'fire',
  'interact',
  'pause',
  'resume',
  'join',
  'leave',
  'state_sync',
  'host_change',
  'bridge_authorize',
  'bridge_authenticate',
  'bridge_get_channel',
  'bridge_get_participants',
  'bridge_open_link',
  'bridge_set_activity',
  'bridge_data_send',
  'bridge_storage_set',
  'bridge_storage_remove',
  'bridge_save_file',
  'bridge_subscribe',
  'bridge_unsubscribe',
  'bridge_other',
  'tick',
  'other',
] as const;
export type TickInput = (typeof TICK_INPUTS)[number];
export interface CrashTick {
  t: number;
  src: TickSource;
  input: TickInput;
}

/** An input as the closed enum; anything else is `other` and its text is never copied. */
export const asTickInput = (v: unknown): TickInput => TICK_INPUTS.find((i) => i === v) ?? 'other';

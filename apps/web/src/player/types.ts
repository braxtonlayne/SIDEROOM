import type { ComponentType } from 'react';
import { EQ_FREQS, EQ_MAX_DB } from '../music/eq/presets';

/** One playable item. `src` is a URL the `<audio>` element can stream (a presigned bucket URL, wave 27). */
export interface PlayerTrack {
  id: string;
  title: string;
  artist?: string;
  album?: string;
  src: string;
  /** Cover art URL (a `covers/<hash>.jpg` presign); shown in the lock screen and the skins. */
  artworkUrl?: string;
  /** Length hint in seconds, used until the element reports the real one. */
  durationSec?: number;
  /**
   * Fetch a fresh URL right before the track starts (a presign can expire while a long queue plays).
   * The result replaces `src` for this play only.
   */
  resolveSrc?: (track: PlayerTrack) => Promise<string>;
  /**
   * The file's container as the library knows it (`mp3`, `flac`, ...), for the skins' format readouts
   * (MUS-SKN-001). Absent when nobody knows; a skin then leaves that readout dark rather than guess.
   */
  format?: string;
}

export type PlayerStatus = 'idle' | 'loading' | 'playing' | 'paused' | 'ended' | 'error';
export type PlayerSkin =
  'retro' | 'pill' | 'library' | 'valve' | 'tidepool' | 'circuit' | 'study' | 'stack';
export type PlayerDock = 'bottom-end' | 'bottom-start' | 'top-end' | 'top-start';
export type RepeatMode = 'off' | 'all' | 'one';

export const PLAYER_SKINS: readonly PlayerSkin[] = [
  'retro',
  'pill',
  'library',
  // MUS-SKN-001: the backdrop skins (decorative artwork with only the live controls laid over it), lazy chunks.
  'valve',
  'tidepool',
  'circuit',
  'study',
  'stack',
];
export const PLAYER_DOCKS: readonly PlayerDock[] = [
  'bottom-end',
  'bottom-start',
  'top-end',
  'top-start',
];
export const REPEAT_MODES: readonly RepeatMode[] = ['off', 'all', 'one'];

/**
 * Ten EQ bands in Hz (ISO octave centres); the first is a low shelf and the last a high shelf. One definition
 * for the player and the library's equalizer (`music/eq/presets.ts`), whose chain the player's graph uses.
 */
export const EQ_BANDS = EQ_FREQS;
export const EQ_RANGE_DB = EQ_MAX_DB;
export const CROSSFADE_MAX_SEC = 12;

/** What the "Full Library Manager" skin renders in its content slot (music-lib registers it). */
export type LibraryPanelComponent = ComponentType;

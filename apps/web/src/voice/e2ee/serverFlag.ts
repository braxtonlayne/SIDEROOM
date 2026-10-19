/**
 * What the server last said about call encryption: `sr_e2ee` in `VOICE_SERVER_UPDATE` (its
 * `voice.e2ee`). `null` until the first join. The controller writes it; Settings > Advanced reads it so
 * it does not claim calls are end-to-end encrypted on a server that turned it off.
 */
import { create } from 'zustand';

export const useServerE2ee = create<{ on: boolean | null }>(() => ({ on: null }));

export function noteServerE2ee(on: boolean): void {
  useServerE2ee.setState({ on });
}

// The only part of the player that ships in the entrance bundle: a flag saying "a Cinema item is loaded", so the
// app root can mount the (lazy) mini player host exactly when there is something to show. The session sets it.
import { create } from 'zustand';

export const useMiniPlayerActive = create<{ active: boolean }>(() => ({ active: false }));
export const setMiniPlayerActive = (active: boolean) => useMiniPlayerActive.setState({ active });

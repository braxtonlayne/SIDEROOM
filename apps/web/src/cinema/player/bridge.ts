// The only part of the player that ships in the entrance bundle: a flag saying "a Cinema item is loaded", so the
// app root can mount the (lazy) mini player host exactly when there is something to show. The session sets it.
// `expandHref` lets a watch party point the mini player's "expand" back at the party theatre.
import { create } from 'zustand';

export const useMiniPlayerActive = create<{ active: boolean; expandHref: string | null }>(() => ({
  active: false,
  expandHref: null,
}));
export const setMiniPlayerActive = (active: boolean) => useMiniPlayerActive.setState({ active });
export const setMiniExpandHref = (expandHref: string | null) =>
  useMiniPlayerActive.setState({ expandHref });

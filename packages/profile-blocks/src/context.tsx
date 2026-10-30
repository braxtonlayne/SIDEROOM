import { createContext, useContext, type ReactNode } from 'react';
import type { CdnResolver, Translate } from './types';

interface Env {
  t: Translate;
  cdn: CdnResolver;
}

const Ctx = createContext<Env | null>(null);

/** The host supplies translation and image addresses; the blocks import neither. */
export function BlocksEnv({ t, cdn, children }: Env & { children: ReactNode }) {
  return <Ctx.Provider value={{ t, cdn }}>{children}</Ctx.Provider>;
}

export function useEnv(): Env {
  const env = useContext(Ctx);
  if (!env) throw new Error('profile-blocks: wrap the renderer in <BlocksEnv>');
  return env;
}

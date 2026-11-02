import { createContext, useContext, type ComponentType, type ReactNode } from 'react';
import type { CdnResolver, Translate } from './types';

/**
 * Draws a payment link (a Bitcoin or Lightning URI) as a QR code. The host fills it: the package imports no encoder,
 * because it is in the public `/~handle` bundle (D-385) and the encoder belongs to the app alone (FU-18).
 */
export type TipQr = ComponentType<{ value: string; label: string }>;

interface Env {
  t: Translate;
  cdn: CdnResolver;
  /** Absent on the public profile and the mini popout: tip links stay text there. */
  tipQr?: TipQr;
}

const Ctx = createContext<Env | null>(null);

/** The host supplies translation and image addresses (and, in the signed-in app, the QR drawer); the blocks import none. */
export function BlocksEnv({ t, cdn, tipQr, children }: Env & { children: ReactNode }) {
  return <Ctx.Provider value={{ t, cdn, tipQr }}>{children}</Ctx.Provider>;
}

export function useEnv(): Env {
  const env = useContext(Ctx);
  if (!env) throw new Error('profile-blocks: wrap the renderer in <BlocksEnv>');
  return env;
}

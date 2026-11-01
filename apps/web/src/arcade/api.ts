// Typed fetchers for the Arcade routes. Components never build URLs. The Operator's calls are in adminApi.ts.
import { apiFetch } from '../api/client';
import type { ArcadeFront, Cartridge, CartridgeInput } from './types';

const enc = encodeURIComponent;
const NO_RETRY = { retries: 0 } as const;

export interface ArcadeApi {
  front(): Promise<ArcadeFront>;
  setNotify(entryId: string, notify: boolean): Promise<void>;
  cartridges(guildId: string): Promise<Cartridge[]>;
  addCartridge(guildId: string, input: CartridgeInput): Promise<Cartridge>;
  removeCartridge(guildId: string, cartridgeId: string): Promise<void>;
}

export const httpArcadeApi: ArcadeApi = {
  front: () => apiFetch('/arcade', NO_RETRY),
  setNotify: async (id, notify) => {
    await apiFetch(`/arcade/coming-soon/${enc(id)}/notify`, {
      method: 'PUT',
      json: { notify },
      ...NO_RETRY,
    });
  },
  cartridges: (g) => apiFetch(`/guilds/${enc(g)}/arcade/cartridges`, NO_RETRY),
  addCartridge: (g, input) =>
    apiFetch(`/guilds/${enc(g)}/arcade/cartridges`, { method: 'POST', json: input, ...NO_RETRY }),
  removeCartridge: async (g, id) => {
    await apiFetch(`/guilds/${enc(g)}/arcade/cartridges/${enc(id)}`, {
      method: 'DELETE',
      ...NO_RETRY,
    });
  },
};

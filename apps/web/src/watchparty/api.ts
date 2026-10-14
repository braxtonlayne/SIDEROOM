// Typed fetchers for the Watch Party routes (docs/specs/watch-parties.md section 7.2). Components and the sync
// controller never build URLs. `PartyApi` is the seam the controller takes, so tests pass a fake server.
import { apiFetch } from '../api/client';
import type {
  CommandInput,
  ControlMode,
  CreateInput,
  HeartbeatInput,
  HeartbeatResult,
  Party,
  PartySettings,
  Playback,
} from './types';
import { t } from '../i18n';

const enc = encodeURIComponent;
const NO_RETRY = { retries: 0 } as const;

export interface PartyApi {
  time(): Promise<{ server_time_ms: number }>;
  get(id: string): Promise<Party>;
  mine(): Promise<Party>;
  inChannel(channelId: string): Promise<Party>;
  create(input: CreateInput): Promise<Party>;
  join(id: string, inviteCode?: string | null): Promise<Party>;
  leave(id: string): Promise<void>;
  end(id: string): Promise<void>;
  updateSettings(id: string, s: Partial<PartySettings>): Promise<Party>;
  command(id: string, c: CommandInput): Promise<Playback>;
  changeMedia(
    id: string,
    m: { title_id: string; episode_id?: string | null; variant_id?: string | null },
  ): Promise<Party>;
  queueAdd(
    id: string,
    m: { title_id: string; episode_id?: string | null; variant_id?: string | null },
  ): Promise<Party>;
  queueMove(id: string, item: string, position: number): Promise<Party>;
  queueRemove(id: string, item: string): Promise<Party>;
  next(id: string): Promise<Party>;
  heartbeat(id: string, b: HeartbeatInput): Promise<HeartbeatResult>;
  react(id: string, emoji: string): Promise<void>;
  say(id: string, content: string): Promise<void>;
  transferHost(id: string, userId: string): Promise<Party>;
  setController(id: string, userId: string, controller: boolean): Promise<Party>;
  kick(id: string, userId: string): Promise<void>;
}

const base = (id: string) => `/watch-parties/${enc(id)}`;

export const httpPartyApi: PartyApi = {
  time: () => apiFetch(`/watch-parties/time`, NO_RETRY),
  get: (id) => apiFetch(base(id), NO_RETRY),
  mine: () => apiFetch(`/watch-parties/me`, NO_RETRY),
  inChannel: (c) => apiFetch(`/channels/${enc(c)}/watch-party`, NO_RETRY),
  create: (json) => apiFetch(`/watch-parties`, { method: 'POST', json, ...NO_RETRY }),
  join: (id, inviteCode) =>
    apiFetch(`${base(id)}/join`, {
      method: 'POST',
      json: { invite_code: inviteCode ?? undefined },
      ...NO_RETRY,
    }),
  leave: async (id) => {
    await apiFetch(`${base(id)}/leave`, { method: 'POST', ...NO_RETRY });
  },
  end: async (id) => {
    await apiFetch(base(id), { method: 'DELETE', ...NO_RETRY });
  },
  updateSettings: (id, json) => apiFetch(base(id), { method: 'PATCH', json, ...NO_RETRY }),
  command: (id, json) => apiFetch(`${base(id)}/playback`, { method: 'POST', json, ...NO_RETRY }),
  changeMedia: (id, json) => apiFetch(`${base(id)}/media`, { method: 'PUT', json, ...NO_RETRY }),
  queueAdd: (id, json) => apiFetch(`${base(id)}/queue`, { method: 'POST', json, ...NO_RETRY }),
  queueMove: (id, item, position) =>
    apiFetch(`${base(id)}/queue/${enc(item)}`, {
      method: 'PATCH',
      json: { position },
      ...NO_RETRY,
    }),
  queueRemove: (id, item) =>
    apiFetch(`${base(id)}/queue/${enc(item)}`, { method: 'DELETE', ...NO_RETRY }),
  next: (id) => apiFetch(`${base(id)}/next`, { method: 'POST', ...NO_RETRY }),
  heartbeat: (id, json) => apiFetch(`${base(id)}/heartbeat`, { method: 'POST', json, ...NO_RETRY }),
  react: async (id, emoji) => {
    await apiFetch(`${base(id)}/reactions`, { method: 'POST', json: { emoji }, ...NO_RETRY });
  },
  say: async (id, content) => {
    await apiFetch(`${base(id)}/messages`, { method: 'POST', json: { content }, ...NO_RETRY });
  },
  transferHost: (id, user_id) =>
    apiFetch(`${base(id)}/host`, { method: 'POST', json: { user_id }, ...NO_RETRY }),
  setController: (id, userId, controller) =>
    apiFetch(`${base(id)}/controllers/${enc(userId)}`, {
      method: 'PUT',
      json: { controller },
      ...NO_RETRY,
    }),
  kick: async (id, userId) => {
    await apiFetch(`${base(id)}/members/${enc(userId)}`, { method: 'DELETE', ...NO_RETRY });
  },
};

/** Labels resolve when read, so they follow the active language. */
export const CONTROL_LABEL: Record<ControlMode, string> = {
  get host() {
    return t('watchparty.api.controlHost');
  },
  get controllers() {
    return t('watchparty.api.controlControllers');
  },
  get everyone() {
    return t('watchparty.api.controlEveryone');
  },
};

/** The link that opens a party (private parties carry their invite code). */
export function partyLink(origin: string, party: Pick<Party, 'id' | 'invite_code'>): string {
  const code = party.invite_code ? `?code=${enc(party.invite_code)}` : '';
  return `${origin}/cinema/party/${enc(party.id)}${code}`;
}

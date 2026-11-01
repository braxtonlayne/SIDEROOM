// ARC-GST-002: the member's calls for a guest link, and where this tab remembers its key. The key is made here and is
// never sent to the server (it rides in the link's fragment); a reload of this tab keeps it in sessionStorage so the
// bridge can seat again, and nothing else ever reads it.
import { apiFetch } from '../api/client';
import {
  guestLinkPath,
  guestLinksPath,
  type GuestLinkCreated,
} from '../public/guest-play/contract';
import { buildGuestLink, generateGameKey } from '../public/guest-play/link';

const NO_RETRY = { retries: 0 } as const;

export interface GuestLinkApi {
  create(instanceId: string): Promise<GuestLinkCreated>;
  revoke(instanceId: string, lobbyToken: string): Promise<void>;
}

export const httpGuestLinkApi: GuestLinkApi = {
  create: (instanceId) =>
    apiFetch<GuestLinkCreated>(guestLinksPath(instanceId), { method: 'POST', ...NO_RETRY }),
  revoke: async (instanceId, lobbyToken) => {
    await apiFetch(guestLinkPath(instanceId, lobbyToken), { method: 'DELETE', ...NO_RETRY });
  },
};

/** What this tab keeps about the link it made for an instance. */
export interface StoredLink {
  lobbyToken: string;
  gameKey: string;
  expiresAt: string;
}

const slot = (instanceId: string) => `sr.guestlink.${instanceId}`;

export function loadLink(instanceId: string, now = Date.now()): StoredLink | null {
  try {
    const raw = sessionStorage.getItem(slot(instanceId));
    if (!raw) return null;
    const j = JSON.parse(raw) as Partial<StoredLink>;
    if (
      typeof j.lobbyToken !== 'string' ||
      typeof j.gameKey !== 'string' ||
      typeof j.expiresAt !== 'string'
    )
      return null;
    if (Date.parse(j.expiresAt) <= now) {
      sessionStorage.removeItem(slot(instanceId));
      return null;
    }
    return { lobbyToken: j.lobbyToken, gameKey: j.gameKey, expiresAt: j.expiresAt };
  } catch {
    return null;
  }
}

export function saveLink(instanceId: string, link: StoredLink): void {
  try {
    sessionStorage.setItem(slot(instanceId), JSON.stringify(link));
  } catch {
    /* private window: the link still works for this page's life */
  }
}

export function forgetLink(instanceId: string): void {
  try {
    sessionStorage.removeItem(slot(instanceId));
  } catch {
    /* nothing stored */
  }
}

/** Make the key in this browser, ask the server for the token, and remember both in this tab. */
export async function createGuestLink(api: GuestLinkApi, instanceId: string): Promise<StoredLink> {
  const gameKey = generateGameKey();
  const made = await api.create(instanceId);
  const link: StoredLink = { lobbyToken: made.lobby_token, gameKey, expiresAt: made.expires_at };
  saveLink(instanceId, link);
  return link;
}

export const linkHref = (origin: string, l: StoredLink): string =>
  buildGuestLink(origin, l.lobbyToken, l.gameKey);

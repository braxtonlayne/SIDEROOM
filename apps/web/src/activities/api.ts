// Typed fetchers for the Activities routes. Components and the host never build URLs.
import { apiFetch } from '../api/client';
import type { AuthenticateResult } from '@sideroom/embedded-app-sdk';
import type { ActivityInstance, ChannelActivity, Shelf } from './types';
import { isTestDeployment } from '../testdeploy/flag';

const enc = encodeURIComponent;
const NO_RETRY = { retries: 0 } as const;

export interface ActivityApi {
  shelf(): Promise<Shelf>;
  inChannel(channelId: string): Promise<ChannelActivity>;
  launch(channelId: string, applicationId: string): Promise<ActivityInstance>;
  join(instanceId: string): Promise<ActivityInstance>;
  leave(instanceId: string): Promise<void>;
  end(instanceId: string): Promise<void>;
}

export const httpActivityApi: ActivityApi = {
  shelf: () => apiFetch('/activities', NO_RETRY),
  inChannel: (c) => apiFetch(`/channels/${enc(c)}/activity`, NO_RETRY),
  launch: (c, application_id) =>
    apiFetch(`/channels/${enc(c)}/activity`, {
      method: 'POST',
      json: { application_id },
      ...NO_RETRY,
    }),
  join: (id) => apiFetch(`/activity-instances/${enc(id)}/join`, { method: 'POST', ...NO_RETRY }),
  leave: async (id) => {
    await apiFetch(`/activity-instances/${enc(id)}/leave`, { method: 'POST', ...NO_RETRY });
  },
  end: async (id) => {
    await apiFetch(`/activity-instances/${enc(id)}`, { method: 'DELETE', ...NO_RETRY });
  },
};

/**
 * Mint the member's proxy token for an instance they take part in (D-399): the sandboxed frame sends it on
 * writes through the app's URL mappings. `null` when the server would not mint one.
 */
export async function mintProxyToken(instanceId: string): Promise<string | null> {
  const r = await apiFetch<{ token?: unknown }>(
    `/activity-instances/${enc(instanceId)}/proxy-token`,
    { method: 'POST', ...NO_RETRY },
  );
  return typeof r?.token === 'string' ? r.token : null;
}

/**
 * Where an Activity is served from, without the final slash: `https://<application-id>.activities.<host>`, its own
 * origin under the site's host, so it can never read the client's cookies or storage (docs/11 section 3). On a
 * TEST deployment (OPS-TDEP-001, `sameHost`) it is `https://<host>/_activity/<application-id>`: the server sends the
 * same sandboxing policy, so the frame's origin is still opaque. `<this>/` is the frame's address and the
 * Activity's implicit OAuth redirect URI (the server builds the same one).
 */
export function activityBase(
  applicationId: string,
  loc: Pick<Location, 'protocol' | 'host'> = window.location,
  sameHost: boolean = isTestDeployment(),
): string {
  return sameHost
    ? `${loc.protocol}//${loc.host}/_activity/${applicationId}`
    : `${loc.protocol}//${applicationId}.activities.${loc.host}`;
}

/** The pieces of `POST /oauth2/authorize` and `GET /oauth2/@me` the host uses, behind a seam for tests. */
export interface OAuthApi {
  /** Approve an authorization for the Activity's application; resolves the `code`. */
  authorizeCode(applicationId: string, scopes: string[], redirectUri: string): Promise<string>;
  /** Check an access token, without the session cookie. */
  tokenInfo(accessToken: string): Promise<AuthenticateResult>;
}

interface MeResponse {
  application: { id: string; name: string };
  scopes: string[];
  expires: string;
  user: {
    id: string;
    username: string | null;
    global_name: string | null;
    avatar: string | null;
  } | null;
}

export const httpOAuthApi: OAuthApi = {
  async authorizeCode(applicationId, scopes, redirectUri) {
    const r = await apiFetch<{ location: string }>('/oauth2/authorize', {
      method: 'POST',
      json: {
        client_id: applicationId,
        scope: scopes.join(' '),
        redirect_uri: redirectUri,
        response_type: 'code',
        authorize: true,
      },
      ...NO_RETRY,
    });
    const code = new URL(r.location, window.location.origin).searchParams.get('code');
    if (!code) throw new Error('The server returned no code.');
    return code;
  },
  async tokenInfo(accessToken) {
    const me = await apiFetch<MeResponse>('/oauth2/@me', {
      headers: { Authorization: `Bearer ${accessToken}` },
      credentials: 'omit',
      ...NO_RETRY,
    });
    return {
      access_token: accessToken,
      user: me.user,
      scopes: me.scopes,
      expires: me.expires,
      application: { id: me.application.id, name: me.application.name },
    };
  },
};

/** Rich Presence assets of an application (any signed-in session may read the list, D-237). */
export interface AssetsApi {
  list(applicationId: string): Promise<{ id: string; name: string }[]>;
}

export const httpAssetsApi: AssetsApi = {
  list: async (applicationId) => {
    const rows = await apiFetch<{ id: string; name: string }[]>(
      `/applications/${enc(applicationId)}/assets`,
      NO_RETRY,
    );
    return rows.map((a) => ({ id: a.id, name: a.name }));
  },
};

/**
 * D-389 (wave 46): the per-object URL of one shelf object for an Activity that declares `byos_read`, minted with the
 * member's own session by the route the Study uses (`GET /shelves/{shelf_id}/objects/{object_hash}/url`). The route
 * checks the grant, the kind rule and every delisting; the URL lives ten minutes at most.
 */
export function mintShelfObjectUrl(
  shelfId: string,
  objectHash: string,
): Promise<{ url: string; expires_at: string }> {
  return apiFetch(`/shelves/${enc(shelfId)}/objects/${enc(objectHash)}/url`, NO_RETRY);
}

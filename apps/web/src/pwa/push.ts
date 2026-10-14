import { ApiError, apiFetch } from '../api/client';

/**
 * Web Push client (XC-PWA-004). The backend has no push routes yet, so this is a typed client for the
 * contract the UI needs, behind a graceful "unsupported" state:
 *   GET    /api/v1/push/vapid-key                     -> { public_key }
 *   POST   /api/v1/users/@me/push-subscriptions       <- PushSubscriptionBody
 *   DELETE /api/v1/users/@me/push-subscriptions       <- { endpoint }
 * A 404/405/501 from any of them means the server does not do push, and the UI says so.
 */
export interface PushSubscriptionBody {
  endpoint: string;
  keys: { p256dh: string; auth: string };
  /** Free-text device label shown in the list of devices. */
  device_name: string;
}

export type PushUnsupportedReason = 'no-browser-support' | 'no-server-support';
export type PushStatus =
  | { kind: 'unsupported'; reason: PushUnsupportedReason }
  | { kind: 'blocked' }
  | { kind: 'off'; permission: NotificationPermission }
  | { kind: 'on' };

const DEVICE_KEY = 'sr.push.device';

export function browserSupportsPush(): boolean {
  return (
    typeof navigator !== 'undefined' &&
    'serviceWorker' in navigator &&
    typeof window !== 'undefined' &&
    'PushManager' in window &&
    typeof Notification !== 'undefined'
  );
}

const missing = (e: unknown) =>
  e instanceof ApiError && (e.status === 404 || e.status === 405 || e.status === 501);

/** The server's VAPID public key, or null when the server has no push support. */
export async function fetchVapidKey(): Promise<string | null> {
  try {
    const r = await apiFetch<{ public_key?: string }>('/api/v1/push/vapid-key', { retries: 0 });
    return r.public_key ?? null;
  } catch (e) {
    if (missing(e)) return null;
    throw e;
  }
}

export function urlBase64ToBytes(b64: string): Uint8Array {
  const pad = '='.repeat((4 - (b64.length % 4)) % 4);
  const raw = atob((b64 + pad).replace(/-/g, '+').replace(/_/g, '/'));
  return Uint8Array.from(raw, (c) => c.charCodeAt(0));
}

const bufToB64 = (buf: ArrayBuffer | null): string => {
  if (!buf) return '';
  let s = '';
  for (const b of new Uint8Array(buf)) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
};

export function toBody(sub: PushSubscription, deviceName: string): PushSubscriptionBody {
  return {
    endpoint: sub.endpoint,
    keys: { p256dh: bufToB64(sub.getKey('p256dh')), auth: bufToB64(sub.getKey('auth')) },
    device_name: deviceName,
  };
}

async function currentSubscription(): Promise<PushSubscription | null> {
  const reg = await navigator.serviceWorker.getRegistration();
  return (await reg?.pushManager.getSubscription()) ?? null;
}

/** Where this device stands. Never throws. */
export async function pushStatus(): Promise<PushStatus> {
  if (!browserSupportsPush()) return { kind: 'unsupported', reason: 'no-browser-support' };
  if (Notification.permission === 'denied') return { kind: 'blocked' };
  try {
    if ((await currentSubscription()) && localStorage.getItem(DEVICE_KEY) === '1')
      return { kind: 'on' };
  } catch {
    /* fall through to off */
  }
  return { kind: 'off', permission: Notification.permission };
}

/** Ask permission (call from a user gesture), subscribe and register with the server. */
export async function enablePush(deviceName: string): Promise<PushStatus> {
  if (!browserSupportsPush()) return { kind: 'unsupported', reason: 'no-browser-support' };
  const key = await fetchVapidKey();
  if (!key) return { kind: 'unsupported', reason: 'no-server-support' };
  const permission = await Notification.requestPermission();
  if (permission === 'denied') return { kind: 'blocked' };
  if (permission !== 'granted') return { kind: 'off', permission };
  const reg = await navigator.serviceWorker.ready;
  const sub =
    (await reg.pushManager.getSubscription()) ??
    (await reg.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: urlBase64ToBytes(key) as BufferSource,
    }));
  try {
    await apiFetch('/api/v1/users/@me/push-subscriptions', {
      method: 'POST',
      json: toBody(sub, deviceName),
    });
  } catch (e) {
    await sub.unsubscribe().catch(() => false);
    if (missing(e)) return { kind: 'unsupported', reason: 'no-server-support' };
    throw e;
  }
  try {
    localStorage.setItem(DEVICE_KEY, '1');
  } catch {
    /* ignore */
  }
  return { kind: 'on' };
}

/** Turn push off for this device only: unsubscribe locally and tell the server. */
export async function disablePush(): Promise<PushStatus> {
  try {
    localStorage.removeItem(DEVICE_KEY);
  } catch {
    /* ignore */
  }
  if (!browserSupportsPush()) return { kind: 'unsupported', reason: 'no-browser-support' };
  const sub = await currentSubscription();
  if (sub) {
    try {
      await apiFetch('/api/v1/users/@me/push-subscriptions', {
        method: 'DELETE',
        json: { endpoint: sub.endpoint },
      });
    } catch (e) {
      if (!missing(e)) throw e;
    }
    await sub.unsubscribe().catch(() => false);
  }
  return { kind: 'off', permission: Notification.permission };
}

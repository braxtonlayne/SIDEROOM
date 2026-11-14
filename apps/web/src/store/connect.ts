// Wires the gateway client to the store. One connection per page.
import { reportUnauthorized } from '../api/client';
import { CloseCode, GatewayClient, type GatewayOptions } from '../gateway';
import { useStore } from './store';

let client: GatewayClient | null = null;
/** Features that need the live connection outside the Lounge (a watch party) hold it open. */
let holds = 0;
let closeWanted = false;

export type ConnectOptions = Partial<Omit<GatewayOptions, 'onDispatch' | 'onStatus'>>;

/**
 * How long READY and the GUILD_CREATEs that follow it are collected before they reach the store as one update
 * (K-22). An account in 60 servers got 61 store updates, and every screen rendered after each of them.
 */
export const BURST_MS = 30;

let burst: { event: string; data: unknown }[] = [];
let burstTimer: ReturnType<typeof setTimeout> | undefined;

/** Hand the collected READY / GUILD_CREATE dispatches to the store now. */
export function flushBurst(): void {
  clearTimeout(burstTimer);
  burstTimer = undefined;
  const batch = burst;
  burst = [];
  if (!batch.length) return;
  const many = batch.filter((b) => b.event === 'GUILD_CREATE').length > 1;
  useStore.getState().applyDispatches(batch, { deferPeople: many });
}

/**
 * Events collected into one store update. PRESENCE_UPDATE is among them because a session in many servers
 * gets one per server right after READY (the person's own presence in each), and every one of them was a
 * store update that re-rendered the Lounge: about 4 s of the K-22 measurement with 60 servers.
 */
const BURST_EVENTS = new Set(['READY', 'GUILD_CREATE', 'PRESENCE_UPDATE']);

/**
 * One gateway dispatch on its way into the store. READY, GUILD_CREATE and PRESENCE_UPDATE are collected for
 * a few milliseconds; any other event first flushes them so the order the server sent is the order applied.
 */
export function receiveDispatch(event: string, data: unknown): void {
  if (BURST_EVENTS.has(event)) {
    burst.push({ event, data });
    burstTimer ??= setTimeout(flushBurst, BURST_MS);
    return;
  }
  flushBurst();
  useStore.getState().applyDispatch(event, data);
}

/** What `clientProperties` reads of the browser (a subset of `Navigator`, so a test can hand it plain objects). */
export interface ClientNav {
  userAgent?: string;
  maxTouchPoints?: number;
  userAgentData?: { mobile?: boolean; platform?: string };
}

/**
 * IDENTIFY `properties` (D-702): the platform decides the phone glyph other people see. `os` is `android` or `ios`
 * on a phone or tablet browser, `web` otherwise. `userAgentData.mobile` decides where the browser has it; else the
 * user-agent string (an iPad that claims to be a Mac is told by its touch points). Never the viewport width: a
 * narrow desktop window is not a phone. The user-agent string is not sent.
 */
export function clientProperties(
  nav: ClientNav = typeof navigator === 'undefined' ? {} : navigator,
): {
  os: 'android' | 'ios' | 'web';
  browser: 'sideroom';
  device: 'sideroom';
} {
  const ua = nav.userAgent ?? '';
  const hint = nav.userAgentData?.mobile;
  let os: 'android' | 'ios' | 'web' = 'web';
  if (hint === true)
    os = /ios|iphone|ipad/i.test(nav.userAgentData?.platform ?? ua) ? 'ios' : 'android';
  else if (hint === undefined) {
    if (/android/i.test(ua)) os = 'android';
    else if (
      /iphone|ipad|ipod/i.test(ua) ||
      (/macintosh/i.test(ua) && (nav.maxTouchPoints ?? 0) > 1)
    )
      os = 'ios';
  }
  return { os, browser: 'sideroom', device: 'sideroom' };
}

/** Open the gateway (idempotent). Dispatches and connection status flow into the store. */
export function connectGateway(opts: ConnectOptions = {}): GatewayClient {
  // Somebody wants it open now: an earlier close request (made while a hold kept it open) no longer stands.
  closeWanted = false;
  if (client) return client;
  const { setConnection } = useStore.getState();
  client = new GatewayClient({
    properties: clientProperties(),
    ...opts,
    onDispatch: (event, data) => receiveDispatch(event, data),
    onStatus: setConnection,
    onFatal: (code) => {
      opts.onFatal?.(code);
      // SEC-AUD-002: the server closes a revoked session's socket with 4004. Same exit as a REST 401: the app's
      // handler asks who the session is and leaves for the sign-in page when that says "nobody".
      if (code === CloseCode.AUTHENTICATION_FAILED) reportUnauthorized();
    },
  });
  client.connect();
  return client;
}

function close(): void {
  flushBurst();
  client?.close();
  client = null;
  closeWanted = false;
}

/** Close the gateway, unless something holds it open; then it closes when the last hold goes. */
export function disconnectGateway(): void {
  if (holds > 0) {
    closeWanted = true;
    return;
  }
  close();
}

/** Keep the gateway open (connecting it if needed) until the returned release function is called. */
export function holdGateway(): () => void {
  holds++;
  connectGateway();
  let released = false;
  return () => {
    if (released) return;
    released = true;
    holds--;
    if (holds === 0 && closeWanted) close();
  };
}

export const getGateway = (): GatewayClient | null => client;

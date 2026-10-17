// Wires the gateway client to the store. One connection per page.
import { GatewayClient, type GatewayOptions } from '../gateway';
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
 * One gateway dispatch on its way into the store. READY and GUILD_CREATE are collected for a few
 * milliseconds; any other event first flushes them so the order the server sent is the order applied.
 */
export function receiveDispatch(event: string, data: unknown): void {
  if (event === 'READY' || event === 'GUILD_CREATE') {
    burst.push({ event, data });
    burstTimer ??= setTimeout(flushBurst, BURST_MS);
    return;
  }
  flushBurst();
  useStore.getState().applyDispatch(event, data);
}

/** Open the gateway (idempotent). Dispatches and connection status flow into the store. */
export function connectGateway(opts: ConnectOptions = {}): GatewayClient {
  if (client) return client;
  closeWanted = false;
  const { setConnection } = useStore.getState();
  client = new GatewayClient({
    ...opts,
    onDispatch: (event, data) => receiveDispatch(event, data),
    onStatus: setConnection,
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

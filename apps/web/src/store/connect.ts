// Wires the gateway client to the store. One connection per page.
import { GatewayClient, type GatewayOptions } from '../gateway';
import { useStore } from './store';

let client: GatewayClient | null = null;
/** Features that need the live connection outside the Lounge (a watch party) hold it open. */
let holds = 0;
let closeWanted = false;

export type ConnectOptions = Partial<Omit<GatewayOptions, 'onDispatch' | 'onStatus'>>;

/** Open the gateway (idempotent). Dispatches and connection status flow into the store. */
export function connectGateway(opts: ConnectOptions = {}): GatewayClient {
  if (client) return client;
  closeWanted = false;
  const { applyDispatch, setConnection } = useStore.getState();
  client = new GatewayClient({
    ...opts,
    onDispatch: (event, data) => applyDispatch(event, data),
    onStatus: setConnection,
  });
  client.connect();
  return client;
}

function close(): void {
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

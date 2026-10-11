// Wires the gateway client to the store. One connection per page.
import { GatewayClient, type GatewayOptions } from '../gateway';
import { useStore } from './store';

let client: GatewayClient | null = null;

export type ConnectOptions = Partial<Omit<GatewayOptions, 'onDispatch' | 'onStatus'>>;

/** Open the gateway (idempotent). Dispatches and connection status flow into the store. */
export function connectGateway(opts: ConnectOptions = {}): GatewayClient {
  if (client) return client;
  const { applyDispatch, setConnection } = useStore.getState();
  client = new GatewayClient({
    ...opts,
    onDispatch: (event, data) => applyDispatch(event, data),
    onStatus: setConnection,
  });
  client.connect();
  return client;
}

export function disconnectGateway(): void {
  client?.close();
  client = null;
}

export const getGateway = (): GatewayClient | null => client;

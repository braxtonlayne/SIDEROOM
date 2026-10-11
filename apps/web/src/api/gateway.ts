// FND-API-007 stub: gateway client with reconnect backoff and sequence tracking.
export type GatewayHandler = (event: string, data: unknown, seq: number) => void;

export class GatewayClient {
  private ws: WebSocket | null = null;
  private attempt = 0;
  private seq = 0;
  private closed = false;

  constructor(
    private readonly url: string,
    private readonly onDispatch: GatewayHandler,
  ) {}

  get lastSequence(): number {
    return this.seq;
  }

  static backoffMs(attempt: number): number {
    return Math.min(30_000, 1000 * 2 ** attempt);
  }

  connect(): void {
    this.closed = false;
    this.ws = new WebSocket(this.url);
    this.ws.onopen = () => {
      this.attempt = 0;
    };
    this.ws.onmessage = (m) => {
      const p = JSON.parse(String(m.data)) as { op: number; t?: string; d?: unknown; s?: number };
      if (typeof p.s === 'number') this.seq = p.s;
      if (p.op === 0 && p.t) this.onDispatch(p.t, p.d, this.seq);
    };
    this.ws.onclose = () => {
      if (this.closed) return;
      setTimeout(() => this.connect(), GatewayClient.backoffMs(this.attempt++));
    };
  }

  close(): void {
    this.closed = true;
    this.ws?.close();
  }
}

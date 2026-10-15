import type { FetchLike } from './rest';
import type { Socket } from './gateway';

export interface Seen {
  url: string;
  method: string;
  headers: Record<string, string>;
  body: unknown;
}

/** A scripted fetch: replies are consumed in order. */
export function fakeFetch(
  replies: { status: number; body?: unknown; headers?: Record<string, string> }[],
) {
  const seen: Seen[] = [];
  const queue = [...replies];
  const fetch: FetchLike = async (url, init) => {
    seen.push({
      url,
      method: init.method,
      headers: init.headers,
      body: init.body === undefined ? undefined : JSON.parse(init.body),
    });
    const r = queue.shift();
    if (!r) throw new Error('no scripted reply');
    const h = r.headers ?? {};
    return {
      status: r.status,
      headers: { get: (n: string) => h[n.toLowerCase()] ?? null },
      text: async () => (r.body === undefined ? '' : JSON.stringify(r.body)),
    };
  };
  return { fetch, seen };
}

/** An in-memory socket the test plays the server of. */
export class FakeSocket implements Socket {
  sent: Record<string, unknown>[] = [];
  closedWith: number | undefined;
  private msg: ((d: string) => void) | undefined;
  private cls: ((c: number | undefined) => void) | undefined;
  constructor(readonly url: string) {}
  send(data: string): void {
    this.sent.push(JSON.parse(data) as Record<string, unknown>);
  }
  close(code?: number): void {
    this.closedWith = code;
  }
  onMessage(cb: (d: string) => void): void {
    this.msg = cb;
  }
  onClose(cb: (c: number | undefined) => void): void {
    this.cls = cb;
  }
  serverSends(v: unknown): void {
    this.msg?.(JSON.stringify(v));
  }
  serverCloses(code?: number): void {
    this.cls?.(code);
  }
}

export function socketFactory() {
  const sockets: FakeSocket[] = [];
  return {
    sockets,
    createSocket: (url: string) => {
      const s = new FakeSocket(url);
      sockets.push(s);
      return s;
    },
  };
}

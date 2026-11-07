import { setUnauthorizedHandler } from '../api/client';
import type { SocketLike } from '../gateway';
import { connectGateway, disconnectGateway } from './connect';

class FakeSocket implements SocketLike {
  static last: FakeSocket | null = null;
  readyState = 1;
  binaryType = '';
  onopen: SocketLike['onopen'] = null;
  onmessage: SocketLike['onmessage'] = null;
  onclose: SocketLike['onclose'] = null;
  onerror: SocketLike['onerror'] = null;
  constructor(public url: string) {
    FakeSocket.last = this;
  }
  send() {}
  close() {
    this.readyState = 3;
  }
  serverClose(code: number) {
    this.readyState = 3;
    this.onclose?.({ code });
  }
}

const open = () =>
  connectGateway({
    url: 'ws://x/gateway?v=1&encoding=json',
    getToken: () => '',
    createSocket: (u) => new FakeSocket(u),
  });

afterEach(() => {
  disconnectGateway();
  setUnauthorizedHandler(null);
});

describe('gateway close codes (SEC-AUD-002)', () => {
  it('treats 4004 (session revoked) like a 401: the signed-out handler runs', () => {
    const onUnauthorized = vi.fn();
    setUnauthorizedHandler(onUnauthorized);
    open();
    FakeSocket.last!.serverClose(4004);
    expect(onUnauthorized).toHaveBeenCalledTimes(1);
  });

  it('does not sign out on other fatal codes', () => {
    const onUnauthorized = vi.fn();
    setUnauthorizedHandler(onUnauthorized);
    open();
    FakeSocket.last!.serverClose(4014);
    expect(onUnauthorized).not.toHaveBeenCalled();
  });
});

// Entry point: pick a state transport, build the board, start the interface.

import { SideRoomSDK } from '@sideroom/embedded-app-sdk';
import { startApp } from './app';
import { broadcastAdapter } from './loopback';
import { adapterFromSdk, identityFromConfig, type SdkLike } from './sdkState';
import { S } from './strings';
import { Board } from './sync';

async function boot(): Promise<void> {
  const params = new URLSearchParams(location.search);
  const standalone = window.parent === window || params.has('loopback');
  const status = document.getElementById('status');

  if (standalone) {
    // Not inside SIDE ROOM (or asked not to be): tabs of this page on one origin share a board. For the dev
    // server and for looking at the page; `?host=1` makes a tab the host, `?me=<name>` names it.
    const room = params.get('room') ?? 'dev';
    const host = params.get('host') === '1';
    const me = params.get('me') ?? `tab${Math.random().toString(36).slice(2, 8)}`;
    const board = new Board({ me, isHost: host, adapter: broadcastAdapter(room, host) });
    startApp(board, { note: S.loopback });
    return;
  }

  if (status) status.textContent = S.connecting;
  const host = location.hostname.split('.')[0] ?? '';
  const sdk = new SideRoomSDK(params.get('application_id') ?? host);
  const config = await sdk.ready();
  const id = identityFromConfig(config);
  if (!id) throw new Error('The host did not say who this member is.');
  const board = new Board({
    me: id.me,
    isHost: id.isHost,
    adapter: adapterFromSdk(sdk as unknown as SdkLike),
  });
  startApp(board);
}

boot().catch(() => {
  const status = document.getElementById('status');
  if (status) status.textContent = S.failed;
});

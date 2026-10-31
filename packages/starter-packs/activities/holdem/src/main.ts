// Entry point. Inside SIDE ROOM the SDK's data channel carries the table; opened on its own (the vite dev
// server) three players share the page over an in-memory hub so the whole game can be tried alone.

import { SideRoomSDK } from '@sideroom/embedded-app-sdk';
import { PlayerClient } from './client';
import { HostController } from './host';
import { S } from './strings';
import { LoopbackHub, sdkTransport, type SdkLike, type Transport } from './transport';
import { mountTable } from './view';

function app(): HTMLElement {
  const root = document.getElementById('app');
  if (!root) throw new Error('#app is missing');
  return root;
}

function startDev(root: HTMLElement, params: URLSearchParams): void {
  const names = ['Ann', 'Bob', 'Cy'];
  const hub = new LoopbackHub('ann');
  const auto = params.has('seat');
  root.append(Object.assign(document.createElement('p'), { className: 'devnote', textContent: S.devNote }));
  const grid = document.createElement('div');
  grid.className = 'devgrid';
  root.append(grid);
  // The page drains the hub on a timer, like a network.
  setInterval(() => hub.flush(), 10);
  const tps = names.map((n) => hub.connect(n.toLowerCase()));
  const host = new HostController(tps[0] as Transport, {});
  host.start();
  tps.forEach((tp, i) => {
    const mount = document.createElement('div');
    grid.append(mount);
    const client = new PlayerClient(tp);
    mountTable(mount, {
      client,
      host: i === 0 ? host : null,
      defaultName: names[i] as string,
      autoDealMs: 4000,
    });
    client.start();
    if (auto) client.sit(names[i] as string);
  });
}

async function startInRoom(root: HTMLElement, params: URLSearchParams): Promise<void> {
  const sdk = new SideRoomSDK(
    params.get('application_id') ?? location.hostname.split('.')[0] ?? '',
  );
  const config = (await sdk.ready()) as unknown as Record<string, unknown>;
  // act-channel adds the member id and the host to the ready config; until then the SDK's own flag is used.
  const me = String(config.member_id ?? config.user_id ?? '');
  if (!me) throw new Error('The client did not say who this member is.');
  const sdkApi = sdk as unknown as SdkLike;
  const isHost = sdkApi.isHost || config.is_host === true;
  const hostId = isHost ? me : String(config.host_id ?? config.host ?? '');
  const tp = sdkTransport(sdkApi, me, hostId);

  let name = '';
  try {
    const { participants } = await sdk.commands.getInstanceConnectedParticipants();
    const mine = participants.find((p) => p.id === me);
    name = mine?.global_name ?? mine?.username ?? '';
  } catch {
    // The default name is fine.
  }

  const client = new PlayerClient(tp);
  let host: HostController | null = null;
  if (isHost) {
    host = new HostController(tp);
    host.start();
    // A member who leaves the instance is folded and unseated at once.
    const dealer = host;
    sdk.subscribe('ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE', (data) => {
      const list = (data as { participants?: { id: string }[] }).participants;
      if (Array.isArray(list)) dealer.syncParticipants(list.map((p) => p.id));
    });
  }
  mountTable(root, { client, host, defaultName: name || 'Player' });
  client.start();
}

const root = app();
const params = new URLSearchParams(location.search);
if (window.parent === window || params.has('loopback')) startDev(root, params);
else
  startInRoom(root, params).catch(() => {
    root.textContent = 'This table could not connect to the room.';
  });

// The tabletop on two real SDK instances over the SDK's loopback hub (no SFU): what the live spec does in two
// browser contexts, here in one process.
import { LoopbackHub } from '../../../../embedded-app-sdk/src/data.ts';
import { SideRoomSDK } from '../../../../embedded-app-sdk/src/sdk.ts';
import { verifyRecord } from '../src/dice.js';
import { Fog } from '../src/fog.js';
import { gridDims } from '../src/grid.js';
import { K, MAX_ROLLS_KEPT, handleInput, splitChunks, view } from '../src/table.js';
import { identity, runHost, sdkTransport } from '../src/transport.js';

const env = {
  parent: null,
  hostOrigin: 'https://chat.example.org',
  hostWindow: {},
  addEventListener: () => undefined,
  removeEventListener: () => undefined,
};
const MAP = { kind: 'hex', size: 60, ox: 0, oy: 0, width: 600, height: 600, unit: 5, unitName: 'ft' };

function table() {
  const hub = new LoopbackHub('host');
  const make = (id) => {
    const sdk = new SideRoomSDK('7', { env, transport: hub.connect(id) });
    const { self, host } = identity(sdk);
    return sdkTransport(sdk, { me: { id: self, name: id }, hostId: host });
  };
  const host = make('host');
  const alice = make('alice');
  runHost(host, handleInput, { seed: () => 0x1234n, now: () => 1 });
  return { hub, host, alice };
}

describe('the tabletop over sdk.state and sdk.input', () => {
  it('shares the map, a token, and a move in both directions', () => {
    const { host, alice } = table();
    host.set(K.map, MAP);
    expect(view(alice.doc()).map.kind).toBe('hex');
    alice.send({ t: 'token-add', token: { id: 'alice', name: 'Alice', c: 1, r: 1 } });
    expect(view(host.doc()).tokens).toHaveLength(1);
    alice.send({ t: 'move', id: 'alice', c: 4, r: 2 });
    expect(view(host.doc()).tokens[0]).toMatchObject({ c: 4, r: 2 });
    expect(view(alice.doc()).tokens[0]).toMatchObject({ c: 4, r: 2 });
  });
  it('lets a guest not write host keys (the SDK throws; so does the transport first)', () => {
    const { alice } = table();
    expect(() => alice.set(K.map, MAP)).toThrow();
  });
  it('ships a map image and fog as host state a guest can read', () => {
    const { host, alice } = table();
    const data = 'data:image/jpeg;base64,' + 'QUJD'.repeat(9000);
    const chunks = splitChunks(data);
    chunks.forEach((c, i) => host.set(K.img('img001', i), c));
    host.send({ t: 'map', map: { ...MAP, imgId: 'img001', imgChunks: chunks.length } });
    const d = view(alice.doc()).map;
    const dims = gridDims(MAP);
    host.send({ t: 'fog', enc: new Fog(dims.cols, dims.rows, true).encode() });
    const v = view(alice.doc());
    expect(d.imgId).toBe('img001');
    expect(v.image).toBe(data);
    expect(v.fog.hiddenCount()).toBe(dims.cols * dims.rows);
  });
  it('rolls with a visible seed that anyone can verify, and trims the history', () => {
    const { host, alice } = table();
    alice.send({ t: 'roll', expr: '8d6', label: 'fireball' });
    const rec = view(host.doc()).rolls[0];
    expect(rec.seed).toBe('0000000000001234');
    expect(verifyRecord(rec)).toBe(true);
    for (let i = 0; i < MAX_ROLLS_KEPT + 3; i++) alice.send({ t: 'roll', expr: 'd20' });
    expect(view(alice.doc()).rolls).toHaveLength(MAX_ROLLS_KEPT);
  });
});

/** Fakes for the browser media APIs: no real devices, sockets or peer connections in vitest. */
import { vi } from 'vitest';
import type { SocketLike } from '../../gateway/client';
import type { AudioDeps } from '../deps';
import type { EngineDeps, PeerLike } from '../engine';

export class FakeTrack {
  kind = 'audio';
  enabled = true;
  stopped = false;
  private listeners = new Map<string, Set<() => void>>();
  constructor(public label = 'fake mic') {}
  addEventListener(type: string, cb: () => void) {
    if (!this.listeners.has(type)) this.listeners.set(type, new Set());
    this.listeners.get(type)!.add(cb);
  }
  removeEventListener(type: string, cb: () => void) {
    this.listeners.get(type)?.delete(cb);
  }
  stop() {
    this.stopped = true;
  }
  fireEnded() {
    for (const cb of [...(this.listeners.get('ended') ?? [])]) cb();
  }
}

let streamSeq = 0;
export class FakeStream {
  id: string;
  constructor(
    private tracks: FakeTrack[] = [new FakeTrack()],
    id?: string,
  ) {
    this.id = id ?? `stream-${streamSeq++}`;
  }
  getAudioTracks() {
    return this.tracks as unknown as MediaStreamTrack[];
  }
  getTracks() {
    return this.tracks as unknown as MediaStreamTrack[];
  }
}

export class FakeNode {
  connected: unknown[] = [];
  disconnected = false;
  connect(n: unknown) {
    this.connected.push(n);
    return n;
  }
  disconnect() {
    this.disconnected = true;
  }
}
export class FakeGainNode extends FakeNode {
  gain = {
    value: 1,
    setValueAtTime: () => undefined,
    linearRampToValueAtTime: () => undefined,
  };
}
export class FakeAnalyser extends FakeNode {
  fftSize = 2048;
  /** Constant sample amplitude returned by getFloatTimeDomainData (0..1). */
  amplitude = 0;
  getFloatTimeDomainData(buf: Float32Array) {
    buf.fill(this.amplitude);
  }
}

export class FakeAudioContext {
  destination = new FakeNode();
  currentTime = 0;
  sinkId = '';
  closed = false;
  analysers: FakeAnalyser[] = [];
  gains: FakeGainNode[] = [];
  sources: FakeNode[] = [];
  dests: { stream: FakeStream }[] = [];
  oscillators = 0;
  createMediaStreamSource() {
    const n = new FakeNode();
    this.sources.push(n);
    return n;
  }
  createAnalyser() {
    const a = new FakeAnalyser();
    this.analysers.push(a);
    return a;
  }
  createGain() {
    const g = new FakeGainNode();
    this.gains.push(g);
    return g;
  }
  createMediaStreamDestination() {
    const d = Object.assign(new FakeNode(), {
      stream: new FakeStream([new FakeTrack('processed')]),
    });
    this.dests.push(d);
    return d;
  }
  createOscillator() {
    this.oscillators++;
    return Object.assign(new FakeNode(), {
      type: 'sine',
      frequency: { value: 0 },
      start: vi.fn(),
      stop: vi.fn(),
    });
  }
  resume = vi.fn(async () => undefined);
  close = vi.fn(async () => {
    this.closed = true;
  });
  setSinkId = vi.fn(async (id: string) => {
    this.sinkId = id;
  });
}
export class FakeAudioElement {
  muted = false;
  volume = 1;
  autoplay = false;
  srcObject: unknown = null;
  paused = false;
  sinkId = '';
  play = vi.fn(async () => undefined);
  pause() {
    this.paused = true;
  }
  setSinkId = vi.fn(async (id: string) => {
    this.sinkId = id;
  });
}

export interface FakeAudioWorld {
  deps: AudioDeps;
  contexts: FakeAudioContext[];
  elements: FakeAudioElement[];
  gum: ReturnType<typeof vi.fn>;
  streams: FakeStream[];
  emitDeviceChange(): void;
  devices: MediaDeviceInfo[];
}

export function fakeAudio(opts: { webAudio?: boolean } = {}): FakeAudioWorld {
  const world: FakeAudioWorld = {
    contexts: [],
    elements: [],
    streams: [],
    devices: [],
    gum: vi.fn(async () => {
      const s = new FakeStream();
      world.streams.push(s);
      return s as unknown as MediaStream;
    }),
    emitDeviceChange: () => undefined,
    deps: undefined as unknown as AudioDeps,
  };
  const listeners = new Set<() => void>();
  world.emitDeviceChange = () => listeners.forEach((l) => l());
  world.deps = {
    getUserMedia: world.gum as unknown as AudioDeps['getUserMedia'],
    createAudioContext: () => {
      if (opts.webAudio === false) return null;
      const c = new FakeAudioContext();
      world.contexts.push(c);
      return c as unknown as AudioContext;
    },
    createAudioElement: () => {
      const e = new FakeAudioElement();
      world.elements.push(e);
      return e as unknown as HTMLAudioElement;
    },
    enumerateDevices: async () => world.devices,
    onDeviceChange: (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    queryMicPermission: async () => 'granted',
  };
  return world;
}

export class FakeSocket implements SocketLike {
  readyState = 1;
  binaryType = 'blob';
  onopen: SocketLike['onopen'] = null;
  onmessage: SocketLike['onmessage'] = null;
  onclose: SocketLike['onclose'] = null;
  onerror: SocketLike['onerror'] = null;
  sent: { op: number; d?: unknown }[] = [];
  closedWith: number | undefined;
  constructor(public url: string) {}
  send(data: string) {
    this.sent.push(JSON.parse(data) as { op: number; d?: unknown });
  }
  close(code?: number) {
    this.closedWith = code;
    this.readyState = 3;
  }
  /** Server -> client frame. */
  serve(op: number, d?: unknown) {
    this.onmessage?.({ data: JSON.stringify({ op, d }) });
  }
  drop(code = 1006) {
    this.readyState = 3;
    this.onclose?.({ code });
  }
  ops(op: number) {
    return this.sent.filter((f) => f.op === op);
  }
}

export class FakePeer implements PeerLike {
  connectionState = 'new';
  iceGatheringState = 'complete';
  localDescription: { type: string; sdp?: string } | null = null;
  onconnectionstatechange: (() => void) | null = null;
  onicegatheringstatechange: (() => void) | null = null;
  ontrack: PeerLike['ontrack'] = null;
  transceivers: { track: unknown; init: unknown }[] = [];
  offers: ({ iceRestart?: boolean } | undefined)[] = [];
  remote: { type: string; sdp?: string }[] = [];
  closed = false;
  stats: Record<string, unknown>[] = [];
  constructor(public config: unknown) {}
  addTransceiver(track: MediaStreamTrack, init: never) {
    this.transceivers.push({ track, init });
  }
  async createOffer(o?: { iceRestart?: boolean }) {
    this.offers.push(o);
    return {
      type: 'offer',
      sdp: 'v=0\r\nm=audio 9 UDP/TLS/RTP/SAVPF 111\r\na=rtpmap:111 opus/48000/2\r\na=fmtp:111 minptime=10;useinbandfec=1\r\n',
    };
  }
  async createAnswer() {
    return { type: 'answer', sdp: 'answer-sdp' };
  }
  async setLocalDescription(d?: { type: string; sdp?: string }) {
    this.localDescription = d ?? null;
  }
  async setRemoteDescription(d: { type: string; sdp?: string }) {
    this.remote.push(d);
  }
  async getStats() {
    const list = this.stats;
    return { forEach: (cb: (s: Record<string, unknown>) => void) => list.forEach(cb) };
  }
  close() {
    this.closed = true;
  }
  setState(s: string) {
    this.connectionState = s;
    this.onconnectionstatechange?.();
  }
  /** Simulate the SFU forwarding someone's audio. */
  receive(userId: string) {
    const stream = new FakeStream([new FakeTrack('remote')], userId);
    this.ontrack?.({
      track: { kind: 'audio' } as MediaStreamTrack,
      streams: [stream as unknown as MediaStream],
    });
    return stream;
  }
}

export interface FakeEngineWorld {
  deps: EngineDeps;
  sockets: FakeSocket[];
  peers: FakePeer[];
  audio: FakeAudioWorld;
  goOnline(): void;
}

export function fakeEngineDeps(audio = fakeAudio()): FakeEngineWorld {
  const online = new Set<() => void>();
  const world: FakeEngineWorld = {
    sockets: [],
    peers: [],
    audio,
    goOnline: () => online.forEach((c) => c()),
    deps: {
      createSocket: (url) => {
        const s = new FakeSocket(url);
        world.sockets.push(s);
        return s;
      },
      createPeer: (config) => {
        const p = new FakePeer(config);
        world.peers.push(p);
        return p;
      },
      audio: audio.deps,
      onOnline: (cb) => {
        online.add(cb);
        return () => online.delete(cb);
      },
    },
  };
  return world;
}

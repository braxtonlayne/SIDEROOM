/**
 * VOX-E2E-003: start a call's E2EE in the browser: the identity key, the MLS session (main
 * thread) and the media worker, plus the hooks the voice engine attaches to every sender and
 * receiver.
 */
import { loadE2ee, type E2eeApi } from '@sideroom/wasm';
import type { EngineE2ee } from '../engine';
import { attachTransform, peerConfigFor, startE2eeWorker } from './attach';
import { E2eeCall } from './call';
import {
  indexedDbKeyStore,
  loadOrCreateIdentity,
  memoryKeyStore,
  signWithIdentity,
} from './identity';
import type { WorkerOut } from './protocol';
import { resetE2ee, setE2ee, useE2ee } from './store';
import { detectTransformMode, type TransformMode } from './support';

export interface E2eeRuntime {
  engine: EngineE2ee;
  call: E2eeCall;
  stop(): void;
}

export interface E2eeStartContext {
  me: string;
  channelId: string;
  /** op 101. */
  send(d: Record<string, unknown>): boolean;
  roster(): string[];
}

export type E2eeStarter = (ctx: E2eeStartContext) => Promise<E2eeRuntime | 'unsupported'>;

const hex = (b: Uint8Array) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');
const unhex = (s: string) => Uint8Array.from(s.match(/../g) ?? [], (b) => parseInt(b, 16));

/** The engine hooks for one worker. */
export function engineHooks(
  worker: Worker | { postMessage(m: unknown, t?: Transferable[]): void },
  mode: TransformMode,
): EngineE2ee {
  const w = worker as Worker;
  // ACT-CH-001: activity data messages are sealed and opened in the worker, which holds the keys.
  let nextId = 1;
  const waiting = new Map<number, (b: Uint8Array | null) => void>();
  if (typeof w.addEventListener === 'function') {
    w.addEventListener('message', (ev: MessageEvent<WorkerOut>) => {
      if (ev.data.type !== 'dataDone') return;
      waiting.get(ev.data.id)?.(ev.data.bytes);
      waiting.delete(ev.data.id);
    });
  }
  const ask = (msg: { type: 'dataSeal' | 'dataOpen'; sender?: string; bytes: Uint8Array }) =>
    new Promise<Uint8Array | null>((resolve) => {
      const id = nextId++;
      waiting.set(id, resolve);
      w.postMessage({ ...msg, id });
    });
  return {
    peerConfig: peerConfigFor(mode),
    sealData: (bytes) => ask({ type: 'dataSeal', bytes }),
    openData: (sender, bytes) => ask({ type: 'dataOpen', sender, bytes }),
    attachSender: (sender, kind) => {
      if (sender)
        attachTransform(
          sender,
          w,
          { direction: 'send', mime: kind === 'audio' ? 'audio/opus' : 'video/*' },
          mode,
        );
    },
    attachReceiver: (receiver, kind, userId) => {
      if (receiver)
        attachTransform(
          receiver,
          w,
          { direction: 'recv', mime: kind === 'audio' ? 'audio/opus' : 'video/*', sender: userId },
          mode,
        );
    },
  };
}

/** Publish the call's state to the UI store, with the D-250 pairwise codes. */
export function bindToStore(
  call: E2eeCall,
  api: E2eeApi,
  me: string,
  selfKey: Uint8Array,
): () => void {
  setE2ee({ active: true, call: call.state, selfKey: hex(selfKey), pairwise: {} });
  return call.subscribe((s) => {
    const pairwise: Record<string, string> = {};
    for (const m of s.members) {
      if (m.userId !== me)
        pairwise[m.userId] = api.pairwiseCode(me, selfKey, m.userId, unhex(m.identityKey));
    }
    setE2ee({ call: s, pairwise });
  });
}

/** e2e builds only: let the browser tests read the call's E2EE state (codes, frame counters). */
if (import.meta.env.VITE_SR_E2E === '1')
  (globalThis as unknown as { __srE2ee?: () => unknown }).__srE2ee = () => useE2ee.getState();

export const startBrowserE2ee: E2eeStarter = async (ctx) => {
  const mode = detectTransformMode();
  if (mode === 'none') return 'unsupported';
  const api = await loadE2ee();
  let store;
  try {
    store = indexedDbKeyStore();
  } catch {
    store = memoryKeyStore();
  }
  const identity = await loadOrCreateIdentity(ctx.me, store);
  const worker = startE2eeWorker();
  worker.addEventListener('message', (ev: MessageEvent<WorkerOut>) => {
    if (ev.data.type === 'stats' && ev.data.frames) setE2ee({ frames: ev.data.frames });
    else if (ev.data.type === 'error') setE2ee({ workerError: ev.data.message });
  });
  const call = new E2eeCall({
    api,
    me: ctx.me,
    channelId: ctx.channelId,
    identity: {
      rawPublicKey: identity.rawPublicKey,
      sign: (m) => signWithIdentity(identity, m),
    },
    send: ctx.send,
    roster: ctx.roster,
    worker: worker as unknown as { postMessage(m: unknown, t?: Transferable[]): void },
    setTimer: (fn, ms) => setTimeout(fn, ms),
    clearTimer: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
  });
  const unbind = bindToStore(call, api, ctx.me, identity.rawPublicKey);
  await call.start();
  return {
    engine: engineHooks(worker, mode),
    call,
    stop: () => {
      unbind();
      call.stop();
      worker.terminate();
      resetE2ee();
    },
  };
};

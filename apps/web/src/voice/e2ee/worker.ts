/**
 * VOX-E2E-003: the E2EE media worker. One per call; it holds the call's media keys (inside
 * WASM) and transforms every encoded frame of every track, sent and received.
 */
import { loadE2ee } from '@sideroom/wasm';
import { handleRtcTransform, handleWorkerMessage, type RtcTransformerLike } from './handler';
import { E2eeMedia } from './media';
import type { WorkerIn, WorkerOut } from './protocol';

/** The parts of DedicatedWorkerGlobalScope used here (the app is typed against the DOM lib). */
const ctx = self as unknown as {
  postMessage(m: WorkerOut): void;
  onmessage: ((ev: MessageEvent<WorkerIn>) => void) | null;
  onrtctransform: ((ev: { transformer: RtcTransformerLike }) => void) | null;
};

const post = (m: WorkerOut) => ctx.postMessage(m);
const queue: WorkerIn[] = [];
const early: RtcTransformerLike[] = [];
let media: E2eeMedia | null = null;

ctx.onmessage = (ev) => {
  if (media) handleWorkerMessage(media, ev.data, post);
  else queue.push(ev.data);
};

ctx.onrtctransform = (ev) => {
  if (media) handleRtcTransform(media, ev.transformer);
  else early.push(ev.transformer);
};

loadE2ee()
  .then((api) => {
    const m = new E2eeMedia(api);
    media = m;
    for (const msg of queue.splice(0)) handleWorkerMessage(m, msg, post);
    for (const t of early.splice(0)) handleRtcTransform(m, t);
    post({ type: 'ready' });
  })
  .catch((e: unknown) => post({ type: 'error', message: String(e) }));

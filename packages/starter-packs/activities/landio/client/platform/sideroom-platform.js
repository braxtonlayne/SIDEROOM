// @ts-check
// SIDE ROOM (D-57): the Activity runs at <application id>.activities.<host> in the client's iframe. This
// speaks the Embedded App SDK's protocol (SIDEROOM_repo packages/embedded-app-sdk/src/protocol.ts): frames
// `{ sr: 1, kind, … }` with the parent window, only from the origin that framed us. Same flow as Discord: ready,
// authorize (a code our server swaps for a signed ticket), authenticate, participants and their updates, and the
// person's activity line.

import { signIn } from './sign-in.js';

/** @typedef {import('./platform.js').Platform} Platform */
/** @typedef {import('./platform.js').Identity} Identity */

const TIMEOUT_MS = 60_000;

/** Whether this page runs as a SIDE ROOM Activity. @param {Window} win */
export const isSideRoom = (win) => win.parent !== win && /^[^.]+\.activities\./.test(win.location.hostname);

/** @param {any} u @returns {Identity} */
const person = (u) => ({ id: `sideroom-${u.id}`, name: String(u.global_name || u.username || 'Player').slice(0, 24) });

/** @param {Window} win @returns {Platform} */
export function createSideRoomPlatform(win) {
  const applicationId = win.location.hostname.split('.')[0];
  // The host's origin: the page that framed us (as the SDK finds it).
  let hostOrigin = win.location.ancestorOrigins?.[0] ?? '';
  if (!hostOrigin && win.document.referrer)
    try {
      hostOrigin = new URL(win.document.referrer).origin;
    } catch {
      hostOrigin = '';
    }
  const host = win.parent;

  /** @type {Map<string, {resolve: (v: any) => void, reject: (e: Error) => void}>} */
  const pending = new Map();
  /** @type {Map<string, Set<(data: any) => void>>} */
  const events = new Map();
  let seq = 0;
  /** @type {((config: any) => void) | null} */
  let onReady = null;
  /** @type {any} */
  let config = null;

  win.addEventListener('message', (e) => {
    if (e.source !== host) return;
    if (hostOrigin && e.origin !== hostOrigin) return;
    const f = e.data;
    if (!f || typeof f !== 'object' || f.sr !== 1) return;
    if (f.kind === 'ready' && onReady) {
      onReady(f.config);
      onReady = null;
    } else if (f.kind === 'response') {
      const p = pending.get(f.nonce);
      if (!p) return;
      pending.delete(f.nonce);
      if (f.ok) p.resolve(f.data);
      else p.reject(new Error(`${f.error?.message ?? 'failed'} (${f.error?.code ?? '?'})`));
    } else if (f.kind === 'event') events.get(f.event)?.forEach((cb) => cb(f.data));
  });

  /** @param {object} frame */
  const post = (frame) => host.postMessage({ sr: 1, ...frame }, hostOrigin || '*');

  /** @param {string} cmd @param {object} [args] @returns {Promise<any>} */
  function call(cmd, args) {
    const nonce = `${Date.now().toString(36)}-${(seq++).toString(36)}`;
    return new Promise((resolve, reject) => {
      pending.set(nonce, { resolve, reject });
      post({ kind: 'request', nonce, cmd, args });
      setTimeout(() => {
        if (!pending.delete(nonce)) return;
        reject(new Error(`${cmd}: SIDE ROOM did not answer`));
      }, TIMEOUT_MS);
    });
  }

  /** @type {Identity} */
  let me = { id: 'sideroom-unknown', name: 'Player' };
  /** @type {string | null} */
  let ticket = null;
  /** @type {Identity[]} */
  let people = [];
  let lastActivity = '';
  let lastActivityAt = -Infinity;

  return {
    kind: 'sideroom',
    async ready() {
      config = await new Promise((resolve, reject) => {
        onReady = resolve;
        post({ kind: 'handshake', application_id: applicationId });
        setTimeout(() => reject(new Error('SIDE ROOM did not answer the handshake')), 15_000);
      });
      const { code } = await call('authorize', { scope: ['identify'] });
      const signed = await signIn('sideroom', code, win.location.origin);
      await call('authenticate', { access_token: signed.access_token });
      me = signed.user;
      ticket = signed.ticket;
    },
    identity: () => me,
    ticket: () => ticket,
    async participants() {
      try {
        const r = await call('getInstanceConnectedParticipants');
        people = (r?.participants ?? []).filter((/** @type {any} */ p) => !p.bot).map(person);
      } catch {
        people = [me];
      }
      return people;
    },
    onParticipantsChange(cb) {
      /** @param {any} data */
      const h = (data) => {
        people = (data?.participants ?? []).filter((/** @type {any} */ p) => !p.bot).map(person);
        cb(people);
      };
      let set = events.get('ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE');
      if (!set) {
        set = new Set();
        events.set('ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE', set);
        call('subscribe', { event: 'ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE' }).catch(() => {});
      }
      set.add(h);
      return () => set?.delete(h);
    },
    instanceId: () => String(config?.instance_id ?? 'sideroom').replace(/[^A-Za-z0-9_.:-]/g, '_').slice(0, 128),
    setActivity(text) {
      if (text === lastActivity) return;
      // At most every 15 s: the line follows the land share, which changes all the time.
      if (performance.now() - lastActivityAt < 15_000) return;
      lastActivityAt = performance.now();
      lastActivity = text;
      call('setActivity', { activity: { details: 'Playing Land.io', state: text || 'In the lobby' } }).catch(() => {});
    },
  };
}

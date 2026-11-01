// ARC-GST-002 (D-397): the guest side of the Activity SDK bridge. A separate host from `activities/host.ts`, not a
// flag on it: that file reaches the member's account (OAuth, presence, shelves, the voice store) and a guest page may
// not contain it (D-385). This one is built from the allow-list in `capabilities.ts` and has no path to anything but
// the lobby's data lane and roster. A refused command costs one lookup and calls nothing.
import {
  PROTOCOL_VERSION,
  RPC_ERRORS,
  parseFrame,
  b64,
  unb64,
  MAX_DATA_BYTES,
  type Frame,
  type EventName,
  type Participant,
  type RpcErrorBody,
  COMMANDS,
} from '@sideroom/embedded-app-sdk';
import { guestCapabilities, isGuestCommand, isGuestEvent } from './capabilities';
import type { GuestMember } from './contract';

/** The sandboxed frame's origin as a message reports it (opaque). */
export const OPAQUE_ORIGIN = 'null';

/** Same bounds as the member bridge (`activities/host.ts`; `guest.test.ts` pins that they agree). */
export const MAX_REQUEST_CHARS = 12 * 1024 * 1024;
const REQUEST_LIMIT = 4000;
const REQUEST_WINDOW_MS = 10_000;

export function requestProblem(f: { nonce: string; cmd: string; args?: unknown }): string | null {
  if (f.nonce.length === 0 || f.nonce.length > 64) return 'nonce must be 1 to 64 characters.';
  if (f.cmd.length === 0 || f.cmd.length > 64) return 'cmd must be 1 to 64 characters.';
  if (f.args === undefined) return null;
  let size = 0;
  let nodes = 0;
  const stack: unknown[] = [f.args];
  while (stack.length) {
    const v = stack.pop();
    if (++nodes > 20_000) return 'args are too large.';
    if (typeof v === 'string') size += v.length;
    else if (typeof v === 'number' || typeof v === 'boolean') size += 8;
    else if (Array.isArray(v)) stack.push(...v);
    else if (typeof v === 'object' && v !== null) {
      for (const k of Object.keys(v)) {
        if (k === '__proto__') return 'args must not contain __proto__.';
        size += k.length;
      }
      stack.push(...Object.values(v));
    } else if (typeof v === 'function' || typeof v === 'symbol' || typeof v === 'bigint') {
      return 'args must be plain data.';
    }
    if (size > MAX_REQUEST_CHARS) return 'args are too large.';
  }
  return null;
}

/** The lobby's data lane as the host needs it (the page wires it to the guest socket and the game key). */
export interface GuestLane {
  /** Seal and send; false when the lane is closed or the message was dropped. */
  send(bytes: Uint8Array): Promise<boolean>;
  /** Plain bytes another person in the lobby sent; `from` is the id the relay stamped. */
  onMessage(cb: (from: string, bytes: Uint8Array) => void): () => void;
}

export interface GuestHostEnv {
  applicationId: string;
  /** What the lobby's reviewed manifest declares. */
  declared: readonly string[];
  /** This guest's id in the lobby, and the name they play under. */
  /** Read when the game says hello, because the seat is only known once the relay answered. */
  self: () => { id: string; callsign: string };
  roster: () => GuestMember[];
  /** The id the game's host sends under (the member bridge told the lobby); absent: the roster's member seat. */
  hostId?: () => string | null;
  lane: GuestLane;
  post: (frame: Frame) => void;
  /** Origin that frame messages carry. Default [`OPAQUE_ORIGIN`]. */
  senderOrigin?: string;
  now?: () => number;
}

export interface GuestHost {
  handle(data: unknown, origin: string): Promise<void>;
  /** The roster changed: tell a subscribed game. */
  rosterChanged(): void;
  dispose(): void;
}

class Refused extends Error {
  readonly code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
  }
}

const obj = (args: unknown): Record<string, unknown> => {
  if (typeof args !== 'object' || args === null) {
    throw new Refused(RPC_ERRORS.invalidArguments, 'Arguments must be an object.');
  }
  return args as Record<string, unknown>;
};

export function createGuestHost(env: GuestHostEnv): GuestHost {
  const now = env.now ?? Date.now;
  const subscribed = new Set<EventName>();
  const requests: number[] = [];
  let ready = false;
  let off: (() => void) | null = null;
  const caps = guestCapabilities(env.declared);
  const hasData = caps.includes('datachannel');

  const people = (): Participant[] =>
    env.roster().map((m) => ({
      id: m.id,
      username: m.nickname,
      global_name: m.nickname,
      avatar: null,
      bot: false,
    }));

  const emit = (event: EventName, data: unknown) => {
    if (ready && subscribed.has(event))
      env.post({ sr: PROTOCOL_VERSION, kind: 'event', event, data });
  };
  const ok = (nonce: string, data: unknown) =>
    env.post({ sr: PROTOCOL_VERSION, kind: 'response', nonce, ok: true, data });
  const fail = (nonce: string, error: RpcErrorBody) =>
    env.post({ sr: PROTOCOL_VERSION, kind: 'response', nonce, ok: false, error });

  const run = async (cmd: string, args: unknown): Promise<unknown> => {
    // The allow-list comes first: a command that is not on it is never looked at again.
    if (!isGuestCommand(cmd)) {
      if ((COMMANDS as readonly string[]).includes(cmd)) {
        throw new Refused(RPC_ERRORS.notAuthorized, 'This is not available to guests.');
      }
      throw new Refused(RPC_ERRORS.unknownCommand, `Unknown command ${cmd}.`);
    }
    switch (cmd) {
      case 'getInstanceConnectedParticipants':
        return { participants: people() };
      case 'subscribe':
      case 'unsubscribe': {
        const ev = obj(args).event;
        if (typeof ev !== 'string' || !isGuestEvent(ev)) {
          throw new Refused(RPC_ERRORS.notAuthorized, 'That event is not available to guests.');
        }
        if (cmd === 'subscribe') subscribed.add(ev);
        else subscribed.delete(ev);
        return {};
      }
      case 'dataSend': {
        if (!hasData) {
          throw new Refused(
            RPC_ERRORS.notAuthorized,
            'This game did not declare the datachannel capability.',
          );
        }
        const b = obj(args).b;
        if (typeof b !== 'string' || b.length > Math.ceil((MAX_DATA_BYTES * 4) / 3) + 8) {
          throw new Refused(RPC_ERRORS.invalidArguments, 'b must be base64 of at most 16 KiB.');
        }
        let bytes: Uint8Array;
        try {
          bytes = unb64(b);
        } catch {
          throw new Refused(RPC_ERRORS.invalidArguments, 'b must be base64.');
        }
        if (bytes.length > MAX_DATA_BYTES) {
          throw new Refused(RPC_ERRORS.invalidArguments, 'b must be base64 of at most 16 KiB.');
        }
        return { sent: await env.lane.send(bytes) };
      }
    }
  };

  return {
    async handle(data, origin) {
      if (origin !== (env.senderOrigin ?? OPAQUE_ORIGIN)) return;
      const f = parseFrame(data);
      if (!f) return;
      if (f.kind === 'handshake') {
        if (f.application_id !== env.applicationId) return;
        ready = true;
        if (!off && hasData) {
          off = env.lane.onMessage((from, bytes) => emit('DATA_MESSAGE', { from, b: b64(bytes) }));
        }
        const hostId = env.hostId?.() ?? env.roster().find((m) => m.host)?.id ?? env.self().id;
        env.post({
          sr: PROTOCOL_VERSION,
          kind: 'ready',
          config: {
            application_id: env.applicationId,
            // Placeholders: a guest has no instance, channel or server to name (D-397).
            instance_id: 'lobby',
            channel_id: '0',
            guild_id: '0',
            user_id: env.self().id,
            host_id: hostId,
            capabilities: caps,
            guest: true,
            callsign: env.self().callsign,
          },
        });
        return;
      }
      if (f.kind !== 'request') return;
      const problem = requestProblem(f);
      if (problem) {
        fail(f.nonce, { code: RPC_ERRORS.invalidArguments, message: problem });
        return;
      }
      const at = now();
      while (requests.length && at - (requests[0] ?? 0) > REQUEST_WINDOW_MS) requests.shift();
      if (requests.length >= REQUEST_LIMIT) {
        fail(f.nonce, { code: RPC_ERRORS.rateLimited, message: 'Too many requests.' });
        return;
      }
      requests.push(at);
      if (!ready) {
        fail(f.nonce, { code: RPC_ERRORS.notAuthorized, message: 'Call ready() first.' });
        return;
      }
      try {
        ok(f.nonce, await run(f.cmd, f.args));
      } catch (e) {
        if (e instanceof Refused) fail(f.nonce, { code: e.code, message: e.message });
        else fail(f.nonce, { code: RPC_ERRORS.failed, message: 'The request failed.' });
      }
    },
    rosterChanged() {
      emit('ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE', { participants: people() });
    },
    dispose() {
      off?.();
      off = null;
      subscribed.clear();
      ready = false;
    },
  };
}

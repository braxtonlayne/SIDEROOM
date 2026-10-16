// The client side of the Embedded App SDK protocol (DEV-ACT-003): answers one Activity iframe. Pure logic over
// a small `HostEnv`, so it is tested without a DOM. `ActivityFrame` wires it to a real iframe and checks that a
// message comes from that iframe's window before calling `handle`, which checks the origin.
import {
  PROTOCOL_VERSION,
  RPC_ERRORS,
  parseFrame,
  type AuthenticateResult,
  type Frame,
  type Participant,
  type RpcErrorBody,
} from '@sideroom/embedded-app-sdk';
import type { OAuthApi } from './api';
import type { ActivityInstance } from './types';

/** Scopes an Activity may ask for. `email` and the bot scopes are not an Activity's business. */
export const ACTIVITY_SCOPES = ['identify', 'guilds'] as const;

export interface HostEnv {
  /** The origin the Activity runs on; frames from anywhere else are dropped. */
  origin: string;
  applicationId: string;
  instance: () => ActivityInstance;
  channel: () => { id: string; guild_id: string; name: string };
  /** People by id (the live user store), unknown ids omitted. */
  users: (ids: string[]) => Participant[];
  oauth: OAuthApi;
  /** Ask the person; resolves false when they decline. */
  confirm: (
    ask: { kind: 'authorize'; scopes: string[] } | { kind: 'link'; url: string },
  ) => Promise<boolean>;
  openLink: (url: string) => void;
  post: (frame: Frame) => void;
}

class RpcError extends Error {
  readonly code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
  }
}

const invalid = (m: string) => new RpcError(RPC_ERRORS.invalidArguments, m);

function obj(args: unknown): Record<string, unknown> {
  if (typeof args !== 'object' || args === null) throw invalid('Arguments must be an object.');
  return args as Record<string, unknown>;
}

/** `http` and `https` links only; anything else could run script or open a local file. */
export function safeExternalUrl(raw: unknown): string | null {
  if (typeof raw !== 'string' || raw.length > 2048) return null;
  try {
    const u = new URL(raw);
    return u.protocol === 'https:' || u.protocol === 'http:' ? u.toString() : null;
  } catch {
    return null;
  }
}

export interface ActivityHost {
  /** One message from the Activity; `origin` is the sender's. */
  handle(data: unknown, origin: string): Promise<void>;
  /** The instance changed: tell subscribers. */
  participantsChanged(): void;
}

export function createActivityHost(env: HostEnv): ActivityHost {
  let ready = false;
  const subscribed = new Set<string>();

  const ok = (nonce: string, data: unknown) =>
    env.post({ sr: PROTOCOL_VERSION, kind: 'response', nonce, ok: true, data });
  const fail = (nonce: string, error: RpcErrorBody) =>
    env.post({ sr: PROTOCOL_VERSION, kind: 'response', nonce, ok: false, error });

  const participants = (): Participant[] =>
    env.users(env.instance().participants.map((p) => p.user_id));

  const run = async (cmd: string, args: unknown): Promise<unknown> => {
    switch (cmd) {
      case 'authorize': {
        const scopes = obj(args).scope;
        if (
          !Array.isArray(scopes) ||
          scopes.length === 0 ||
          !scopes.every((s) => (ACTIVITY_SCOPES as readonly unknown[]).includes(s))
        ) {
          throw invalid(`scope must list some of: ${ACTIVITY_SCOPES.join(', ')}.`);
        }
        const wanted = [...new Set(scopes as string[])];
        if (!(await env.confirm({ kind: 'authorize', scopes: wanted }))) {
          throw new RpcError(RPC_ERRORS.denied, 'The person declined.');
        }
        const code = await env.oauth.authorizeCode(env.applicationId, wanted, `${env.origin}/`);
        return { code };
      }
      case 'authenticate': {
        const token = obj(args).access_token;
        if (typeof token !== 'string' || token.length === 0 || token.length > 512) {
          throw invalid('access_token must be a string.');
        }
        const info: AuthenticateResult = await env.oauth.tokenInfo(token);
        // A token of another application is not this Activity's business.
        if (info.application.id !== env.applicationId) {
          throw new RpcError(
            RPC_ERRORS.notAuthorized,
            'That token belongs to another application.',
          );
        }
        return info;
      }
      case 'getChannel':
        return env.channel();
      case 'getInstanceConnectedParticipants':
        return { participants: participants() };
      case 'openExternalLink': {
        const url = safeExternalUrl(obj(args).url);
        if (!url) throw invalid('url must be an http or https link.');
        if (!(await env.confirm({ kind: 'link', url }))) return { opened: false };
        env.openLink(url);
        return { opened: true };
      }
      case 'subscribe':
      case 'unsubscribe': {
        const ev = obj(args).event;
        if (ev !== 'ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE') throw invalid('Unknown event.');
        if (cmd === 'subscribe') subscribed.add(ev);
        else subscribed.delete(ev);
        return {};
      }
      default:
        throw new RpcError(RPC_ERRORS.unknownCommand, `Unknown command ${cmd}.`);
    }
  };

  return {
    async handle(data, origin) {
      if (origin !== env.origin) return;
      const f = parseFrame(data);
      if (!f) return;
      if (f.kind === 'handshake') {
        if (f.application_id !== env.applicationId) return;
        ready = true;
        const inst = env.instance();
        env.post({
          sr: PROTOCOL_VERSION,
          kind: 'ready',
          config: {
            application_id: env.applicationId,
            instance_id: inst.id,
            channel_id: inst.channel_id,
            guild_id: inst.guild_id,
          },
        });
        return;
      }
      if (f.kind !== 'request') return;
      if (!ready) {
        fail(f.nonce, { code: RPC_ERRORS.notAuthorized, message: 'Call ready() first.' });
        return;
      }
      try {
        ok(f.nonce, await run(f.cmd, f.args));
      } catch (e) {
        if (e instanceof RpcError) fail(f.nonce, { code: e.code, message: e.message });
        else fail(f.nonce, { code: RPC_ERRORS.failed, message: 'The request failed.' });
      }
    },
    participantsChanged() {
      if (!ready || !subscribed.has('ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE')) return;
      env.post({
        sr: PROTOCOL_VERSION,
        kind: 'event',
        event: 'ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE',
        data: { participants: participants() },
      });
    },
  };
}

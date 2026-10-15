/**
 * Using apps from the web client (DEV-BOT-004/008/010): `POST /interactions`, and waiting for the
 * answer that the gateway sends back to this user (`INTERACTION_SUCCESS`, `INTERACTION_FAILURE`,
 * `INTERACTION_MODAL_CREATE`, `SR_INTERACTION_AUTOCOMPLETE`). Spec: docs/specs/developer-platform.md 3.6.
 */
import { apiFetch, ApiError } from '../../api/client';
import { onDispatch } from '../../store/events';
import type { CommandIndex, WireOption } from './commands';

/** The bot has three seconds to answer; the server reports the failure itself after that. */
export const interactionTimeouts = { waitMs: 5000, autocompleteMs: 4000 };

export interface InteractionRequest {
  /** 2 command, 3 component, 4 autocomplete, 5 modal submit. */
  type: 2 | 3 | 4 | 5;
  /** Optional for components: the server uses the application that wrote the message. */
  application_id?: string;
  channel_id: string;
  guild_id?: string;
  message_id?: string;
  nonce: string;
  data: Record<string, unknown>;
}

export type Outcome =
  | { ok: true; kind: 'success' | 'modal' }
  | { ok: false; reason: 'failed' | 'timeout' | 'rejected'; message?: string };

let counter = 0;
/** A client nonce (at most 64 characters) that ties the answer events to one interaction. */
export function newNonce(): string {
  counter += 1;
  return `i${Date.now().toString(36)}${counter.toString(36)}${Math.random().toString(36).slice(2, 8)}`;
}

export const fetchCommandIndex = (channelId: string) =>
  apiFetch<CommandIndex>(`/channels/${encodeURIComponent(channelId)}/application-commands`, {
    retries: 0,
  });

const post = (req: InteractionRequest) =>
  apiFetch<void>('/interactions', { method: 'POST', json: req, retries: 0 });

function nonceOf(data: unknown): string | null {
  const n = (data as { nonce?: unknown } | null)?.nonce;
  return typeof n === 'string' ? n : null;
}

/** Send an interaction and wait until the app answered it, the server said it failed, or time ran out. */
export function sendInteraction(
  req: InteractionRequest,
  waitMs = interactionTimeouts.waitMs,
): Promise<Outcome> {
  return new Promise((resolve) => {
    let done = false;
    const finish = (o: Outcome) => {
      if (done) return;
      done = true;
      off();
      clearTimeout(timer);
      resolve(o);
    };
    const off = onDispatch((event, data) => {
      if (nonceOf(data) !== req.nonce) return;
      if (event === 'INTERACTION_SUCCESS') finish({ ok: true, kind: 'success' });
      else if (event === 'INTERACTION_MODAL_CREATE') finish({ ok: true, kind: 'modal' });
      else if (event === 'INTERACTION_FAILURE') finish({ ok: false, reason: 'failed' });
    });
    const timer = setTimeout(() => finish({ ok: false, reason: 'timeout' }), waitMs);
    post(req).catch((e: unknown) =>
      finish({
        ok: false,
        reason: 'rejected',
        ...(e instanceof ApiError || e instanceof Error ? { message: e.message } : {}),
      }),
    );
  });
}

export interface AutocompleteChoice {
  name: string;
  value: string | number;
}

/** Ask the app for choices for the focused option. Resolves with an empty list on any failure. */
export function requestAutocomplete(
  base: Omit<InteractionRequest, 'type' | 'nonce' | 'data'>,
  commandId: string,
  options: WireOption[],
  waitMs = interactionTimeouts.autocompleteMs,
): Promise<AutocompleteChoice[]> {
  const nonce = newNonce();
  return new Promise((resolve) => {
    let done = false;
    const finish = (choices: AutocompleteChoice[]) => {
      if (done) return;
      done = true;
      off();
      clearTimeout(timer);
      resolve(choices);
    };
    const off = onDispatch((event, data) => {
      if (event === 'SR_INTERACTION_AUTOCOMPLETE' && nonceOf(data) === nonce) {
        const choices = (data as { choices?: AutocompleteChoice[] }).choices;
        finish(Array.isArray(choices) ? choices : []);
      } else if (event === 'INTERACTION_FAILURE' && nonceOf(data) === nonce) {
        finish([]);
      }
    });
    const timer = setTimeout(() => finish([]), waitMs);
    post({ ...base, type: 4, nonce, data: { id: commandId, options } }).catch(() => finish([]));
  });
}

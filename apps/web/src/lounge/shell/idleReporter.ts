/**
 * Tells the gateway when this browser tab has been left alone (G-51, Discord's idle rule): after
 * {@link IDLE_AFTER_MS} without input it sends op 3 `afk: true`, and on the next input `afk: false`.
 * The server merges the sessions of one account (the user shows idle only when every session is idle) and
 * holds Web Push back while any session is active (D-191), so a forgotten tab does not keep silencing
 * the phone.
 */
import { Op } from '../../gateway/protocol';
import { getGateway } from '../../store';
import { useSelfPresence } from './selfPresence';

/** Idle after ten minutes without input. */
export const IDLE_AFTER_MS = 10 * 60_000;

const INPUT_EVENTS = ['pointerdown', 'pointermove', 'keydown', 'wheel', 'touchstart'] as const;

type Sender = (afk: boolean) => void;

function sendAfk(afk: boolean): void {
  const status = useSelfPresence.getState().status;
  getGateway()?.send(Op.PRESENCE_UPDATE, { status, afk, since: null, activities: [] });
}

/** Starts watching input; returns the stop function. `send` is injectable for tests. */
export function startIdleReporter(send: Sender = sendAfk): () => void {
  let idle = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const arm = () => {
    if (timer !== undefined) clearTimeout(timer);
    timer = setTimeout(() => {
      idle = true;
      send(true);
    }, IDLE_AFTER_MS);
  };
  const onInput = () => {
    if (idle) {
      idle = false;
      send(false);
    }
    arm();
  };
  for (const e of INPUT_EVENTS) window.addEventListener(e, onInput, { passive: true });
  document.addEventListener('visibilitychange', onInput);
  arm();
  return () => {
    if (timer !== undefined) clearTimeout(timer);
    for (const e of INPUT_EVENTS) window.removeEventListener(e, onInput);
    document.removeEventListener('visibilitychange', onInput);
  };
}

import { holdGateway } from '../store/connect';
import { isInVoice, useVoiceSession } from './session';

/**
 * Carry a voice call to a page outside the lounge (wave 37: the Games page, where a co-op host runs the game).
 *
 * The lounge owns the gateway connection (`LiveDataConnection`) and closes it when it unmounts, which ends a call. A
 * link that leads to a page that keeps the call (today only /games) calls `carryCall()` in its click handler, before
 * the lounge unmounts: in a call, that holds the gateway open. The page keeps it with `keepCarriedCall()` (an effect)
 * and lets it go when the last such page unmounts. Out of a call it does nothing, so other navigation is unchanged.
 */
let release: (() => void) | null = null;

export function carryCall(): void {
  if (release || !isInVoice(useVoiceSession.getState())) return;
  release = holdGateway();
}

export function releaseCarriedCall(): void {
  const r = release;
  release = null;
  r?.();
}

let keepers = 0;

/**
 * For the page the call was carried to, in an effect (`useEffect(keepCarriedCall, [])`). The hold is let go a tick after
 * the last keeper unmounts, so React's development double mount does not drop it, and a lounge that mounts next has
 * opened the gateway again by then.
 */
export function keepCarriedCall(): () => void {
  keepers++;
  return () => {
    keepers--;
    setTimeout(() => {
      if (keepers === 0) releaseCarriedCall();
    }, 0);
  };
}

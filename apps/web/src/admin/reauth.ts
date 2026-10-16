// Second-factor window for sensitive admin actions (ADM-STF-009, client side). The server answers a guarded
// action with 403 `SrReauthRequired` (900004) while the 15-minute window is closed; `withReauth` then asks
// the mounted <ReauthHost> (app root, and the admin shell) to collect a code, and retries the original call once.
import { ApiError } from '../api/client';

/** `ErrorCode::SrReauthRequired`. */
export const REAUTH_CODE = 900004;

export const isReauthRequired = (e: unknown): boolean =>
  e instanceof ApiError && e.status === 403 && e.body?.code === REAUTH_CODE;

/** `ErrorCode::SrMfaEnrollmentRequired`: the server demands 2FA and this account has none (D-223). */
export const ENROLLMENT_CODE = 900123;

export const isEnrollmentRequired = (e: unknown): boolean =>
  e instanceof ApiError && e.status === 403 && e.body?.code === ENROLLMENT_CODE;

const enrollmentListeners: Array<() => void> = [];

/** <ReauthHost> subscribes to show the "set up two-factor" notice; returns the unsubscribe function. */
export function onEnrollmentRequired(listener: () => void): () => void {
  enrollmentListeners.push(listener);
  return () => {
    const i = enrollmentListeners.lastIndexOf(listener);
    if (i >= 0) enrollmentListeners.splice(i, 1);
  };
}

/** Resolves true once the window is open, false when the staff member gave up. */
type Prompt = () => Promise<boolean>;
// A stack: the app root mounts a host (so Lounge staff cards can use it) and the admin shell mounts its own;
// the most recently mounted one answers, and unmounting one hands over to the previous.
const prompts: Prompt[] = [];
let pending: Promise<boolean> | null = null;

/** Called by <ReauthHost> on mount; returns the unregister function. */
export function registerReauthPrompt(p: Prompt): () => void {
  prompts.push(p);
  return () => {
    const i = prompts.lastIndexOf(p);
    if (i >= 0) prompts.splice(i, 1);
    if (prompts.length === 0) pending = null;
  };
}

/** One dialog at a time: parallel refused calls wait for the same answer. */
function ask(): Promise<boolean> {
  const prompt = prompts[prompts.length - 1];
  if (!prompt) return Promise.resolve(false);
  pending ??= prompt().finally(() => {
    pending = null;
  });
  return pending;
}

/** Runs `call`; on a re-auth refusal prompts for a second factor and retries once. */
export async function withReauth<T>(call: () => Promise<T>): Promise<T> {
  try {
    return await call();
  } catch (e) {
    // No second factor on the account: the re-auth dialog cannot help, point to Settings instead (G-97).
    if (isEnrollmentRequired(e)) enrollmentListeners[enrollmentListeners.length - 1]?.();
    if (!isReauthRequired(e) || !(await ask())) throw e;
    return call();
  }
}

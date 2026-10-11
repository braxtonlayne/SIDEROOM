import type { MfaMethod } from '../api/entrance';

/**
 * The second-factor methods the server offered at the password step (`mfa_required.methods`, Q83).
 * Kept in sessionStorage between `/login` and `/login/2fa` so a refresh keeps the same choices.
 */
const KEY = 'sr.mfaMethods';
const KNOWN: readonly MfaMethod[] = ['totp', 'webauthn', 'recovery'];

/** Used only when nothing was stored (direct visit to `/login/2fa`): the documented doc 05 set. */
export const DEFAULT_MFA_METHODS: MfaMethod[] = ['totp', 'recovery'];

export function rememberMfaMethods(methods: readonly string[]): void {
  try {
    sessionStorage.setItem(KEY, JSON.stringify(methods.filter(isMfaMethod)));
  } catch {
    /* storage blocked: the step falls back to the default set */
  }
}

export function readMfaMethods(): MfaMethod[] {
  try {
    const parsed: unknown = JSON.parse(sessionStorage.getItem(KEY) ?? 'null');
    if (Array.isArray(parsed)) {
      const methods = parsed.filter(isMfaMethod);
      if (methods.length) return methods;
    }
  } catch {
    /* unreadable or blocked */
  }
  return DEFAULT_MFA_METHODS;
}

export function clearMfaMethods(): void {
  try {
    sessionStorage.removeItem(KEY);
  } catch {
    /* ignore */
  }
}

function isMfaMethod(v: unknown): v is MfaMethod {
  return typeof v === 'string' && (KNOWN as readonly string[]).includes(v);
}

import { passwordStrength } from '@sideroom/ui';

export const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/;
export const USERNAME_RE = /^[a-z0-9_.]{2,32}$/;
export const USERNAME_HINT = '2-32 characters: lowercase letters, numbers, dots and underscores.';
export const MIN_PASSWORD = 10;

export const validateEmail = (v: string) =>
  EMAIL_RE.test(v.trim()) ? null : 'Enter a valid email address.';
export const validateUsername = (v: string) => (USERNAME_RE.test(v) ? null : USERNAME_HINT);
export function validatePassword(v: string): string | null {
  if (v.length < MIN_PASSWORD) return `Use at least ${MIN_PASSWORD} characters.`;
  if (passwordStrength(v) < 2) return 'That password is too easy to guess. Mix it up.';
  return null;
}

/** Only same-origin relative paths may be used as a post-login destination. */
export function safeNext(next: string | undefined): string | null {
  return next && next.startsWith('/') && !next.startsWith('//') && !next.startsWith('/\\')
    ? next
    : null;
}

export function minutesFromSeconds(s: number | null): number {
  return Math.max(1, Math.ceil((s ?? 60) / 60));
}

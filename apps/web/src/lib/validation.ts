import { passwordStrength } from '@sideroom/ui';
import { t } from '../i18n/runtime';

export const EMAIL_RE = /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/;
export const USERNAME_RE = /^[a-z0-9_.]{2,32}$/;
export const MIN_PASSWORD = 10;

// SEC-AUD-040: the messages are translated when a check runs (the auth surface registers its own fragment,
// public/auth/messages.ts).
export const validateEmail = (v: string) =>
  EMAIL_RE.test(v.trim()) ? null : t('entrance.validation.email');
export const validateUsername = (v: string) =>
  USERNAME_RE.test(v) ? null : t('entrance.validation.username');
export function validatePassword(v: string): string | null {
  if (v.length < MIN_PASSWORD) return t('entrance.validation.passwordShort', { min: MIN_PASSWORD });
  if (passwordStrength(v) < 2) return t('entrance.validation.passwordWeak');
  return null;
}

export { safeNext } from './enterApp';

export function minutesFromSeconds(s: number | null): number {
  return Math.max(1, Math.ceil((s ?? 60) / 60));
}

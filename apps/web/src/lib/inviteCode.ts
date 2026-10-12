const CROCKFORD = /[0-9A-HJKMNP-TV-Z]/;
const COMPLETE = /^[0-9A-HJKMNP-TV-Z]{5}-[0-9A-HJKMNP-TV-Z]{5}$/;
const STORE_KEY = 'sr.invite';

/** Uppercase, strip junk, map Crockford look-alikes (I/L to 1, O to 0), cap at 10 chars, insert the dash. */
export function formatInviteCode(raw: string): string {
  const chars = raw
    .toUpperCase()
    .replace(/[IL]/g, '1')
    .replace(/O/g, '0')
    .split('')
    .filter((c) => CROCKFORD.test(c))
    .slice(0, 10)
    .join('');
  return chars.length > 5 ? `${chars.slice(0, 5)}-${chars.slice(5)}` : chars;
}

export const isCompleteInviteCode = (formatted: string) => COMPLETE.test(formatted);

export function holdInviteCode(code: string) {
  try {
    sessionStorage.setItem(STORE_KEY, code);
  } catch {
    /* storage blocked */
  }
}
export function heldInviteCode(): string | null {
  try {
    return sessionStorage.getItem(STORE_KEY);
  } catch {
    return null;
  }
}
export function clearInviteCode() {
  try {
    sessionStorage.removeItem(STORE_KEY);
  } catch {
    /* ignore */
  }
}

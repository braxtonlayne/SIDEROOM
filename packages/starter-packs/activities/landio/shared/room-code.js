// @ts-check
// Room codes (D-59): six characters from an alphabet without the ones people mix up (no I, L, O, 0, 1), typed in
// any case, with or without spaces or dashes. The client and the server normalise with this same function.

export const ALPHABET = 'ABCDEFGHJKMNPQRSTUVWXYZ23456789';
export const CODE_LEN = 6;
const CODE_RE = /^[A-HJ-KM-NP-Z2-9]{6}$/;

/** The code in its one true form ('ABCDEF'), or null when it cannot be one. @param {unknown} s */
export function normalizeCode(s) {
  if (typeof s !== 'string' || s.length > 32) return null;
  const c = s.toUpperCase().replace(/[\s-]/g, '');
  return CODE_RE.test(c) ? c : null;
}

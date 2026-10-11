import { createHmac } from 'node:crypto';

const B32 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

/** RFC 4648 base32 decode (padding, spaces and case ignored). */
export function base32Decode(input: string): Buffer {
  const clean = input.replace(/[=\s]/g, '').toUpperCase();
  let bits = 0;
  let value = 0;
  const out: number[] = [];
  for (const ch of clean) {
    const idx = B32.indexOf(ch);
    if (idx < 0) throw new Error(`invalid base32 character: ${ch}`);
    value = (value << 5) | idx;
    bits += 5;
    if (bits >= 8) {
      out.push((value >>> (bits - 8)) & 0xff);
      bits -= 8;
    }
  }
  return Buffer.from(out);
}

/** RFC 6238 TOTP (HMAC-SHA1, 30 s step, 6 digits by default). */
export function totp(
  secretBase32: string,
  atMs: number = Date.now(),
  step = 30,
  digits = 6,
): string {
  const counter = Math.floor(atMs / 1000 / step);
  const msg = Buffer.alloc(8);
  msg.writeBigUInt64BE(BigInt(counter));
  const h = createHmac('sha1', base32Decode(secretBase32)).update(msg).digest();
  const off = (h[h.length - 1] ?? 0) & 0x0f;
  const bin =
    (((h[off] ?? 0) & 0x7f) << 24) |
    ((h[off + 1] ?? 0) << 16) |
    ((h[off + 2] ?? 0) << 8) |
    (h[off + 3] ?? 0);
  return String(bin % 10 ** digits).padStart(digits, '0');
}

/** Extracts the `secret` parameter from an `otpauth://` URI. */
export function secretFromOtpauth(uri: string): string {
  const secret = new URL(uri).searchParams.get('secret');
  if (!secret) throw new Error('otpauth URI has no secret');
  return secret;
}

/**
 * Waits for the next 30 s window and returns its code. Servers reject a replayed code, so the code used
 * while enrolling cannot be reused to log in within the same window.
 */
export async function nextWindowCode(secretBase32: string, step = 30): Promise<string> {
  const stepMs = step * 1000;
  const wait = stepMs - (Date.now() % stepMs) + 250;
  await new Promise((r) => setTimeout(r, wait));
  return totp(secretBase32);
}

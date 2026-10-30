import { devWasm } from './wasmTools';

const toHex = (buf: ArrayBuffer) =>
  Array.from(new Uint8Array(buf), (b) => b.toString(16).padStart(2, '0')).join('');

export const SHA_ALGORITHMS = ['SHA-1', 'SHA-256', 'SHA-384', 'SHA-512'] as const;

/**
 * Every hash of `bytes`, as [name, lowercase hex]. The SHA family is the browser's `SubtleCrypto`; MD5 and CRC-32
 * come from `sr-tools-wasm` (SubtleCrypto has neither). If the wasm cannot load, the SHA results still return.
 */
export async function computeHashes(bytes: Uint8Array): Promise<[string, string][]> {
  const out: [string, string][] = [];
  try {
    const w = await devWasm();
    out.push(['MD5', w.hashMd5(bytes)], ['CRC-32', w.hashCrc32(bytes)]);
  } catch {
    /* the SHA hashes below still work */
  }
  for (const alg of SHA_ALGORITHMS) {
    out.push([alg, toHex(await crypto.subtle.digest(alg, bytes as BufferSource))]);
  }
  return out;
}

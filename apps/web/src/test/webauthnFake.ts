import { vi } from 'vitest';

const buf = (...bytes: number[]) => new Uint8Array(bytes).buffer;

/** A fake `navigator.credentials` plus `PublicKeyCredential` so WebAuthn code paths run in jsdom. */
export function stubWebAuthn(opts: { reject?: DOMException } = {}) {
  const create = vi.fn(async (...args: [CredentialCreationOptions]) => {
    void args;
    if (opts.reject) throw opts.reject;
    return {
      rawId: buf(1, 2, 3),
      response: { clientDataJSON: buf(4, 5), attestationObject: buf(6, 7, 8) },
    };
  });
  const get = vi.fn(async (...args: [CredentialRequestOptions]) => {
    void args;
    if (opts.reject) throw opts.reject;
    return {
      rawId: buf(1, 2, 3),
      response: {
        clientDataJSON: buf(9),
        authenticatorData: buf(10, 11),
        signature: buf(12),
        userHandle: buf(13, 14),
      },
    };
  });
  vi.stubGlobal('PublicKeyCredential', function PublicKeyCredential() {});
  Object.defineProperty(navigator, 'credentials', {
    configurable: true,
    value: { create, get },
  });
  return { create, get };
}

export function unstubWebAuthn() {
  vi.unstubAllGlobals();
  Object.defineProperty(navigator, 'credentials', { configurable: true, value: undefined });
}

/** Options the way `register/begin` and `login/begin` return them (base64url binary fields). */
export const CREATION_OPTIONS = {
  rp: { id: 'localhost', name: 'SIDE ROOM' },
  user: { id: 'AQID', name: 'alice', displayName: 'Alice' },
  challenge: 'q80',
  pubKeyCredParams: [{ type: 'public-key', alg: -7 }],
  excludeCredentials: [{ type: 'public-key', id: 'BAU' }],
  authenticatorSelection: { residentKey: 'required', userVerification: 'required' },
  attestation: 'none',
};
export const REQUEST_OPTIONS = {
  rpId: 'localhost',
  challenge: 'q80',
  allowCredentials: [],
  userVerification: 'required',
};

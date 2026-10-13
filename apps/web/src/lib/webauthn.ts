// Browser side of the passkey ceremonies (ENT-AUTH-010). The server speaks JSON with base64url
// binary fields (crates/sr-api/src/routes/auth/passkeys.rs); `navigator.credentials` wants
// ArrayBuffers, so options are decoded before the call and the results are encoded after it.
// Hand-rolled instead of PublicKeyCredential.parse*FromJSON so it also works in browsers that lack them.

export const passkeysSupported = (): boolean =>
  typeof window !== 'undefined' &&
  typeof window.PublicKeyCredential === 'function' &&
  !!navigator.credentials;

export function b64urlToBytes(s: string): Uint8Array {
  const pad = '='.repeat((4 - (s.length % 4)) % 4);
  const bin = atob(s.replace(/-/g, '+').replace(/_/g, '/') + pad);
  const out = new Uint8Array(new ArrayBuffer(bin.length));
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export function bytesToB64url(buf: ArrayBuffer | Uint8Array): string {
  const bytes = buf instanceof Uint8Array ? buf : new Uint8Array(buf);
  let bin = '';
  for (const b of bytes) bin += String.fromCharCode(b);
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

interface Descriptor {
  type: 'public-key';
  id: string;
  transports?: AuthenticatorTransport[];
}
export interface CreationOptionsJSON {
  rp: { id: string; name: string };
  user: { id: string; name: string; displayName: string };
  challenge: string;
  pubKeyCredParams: { type: 'public-key'; alg: number }[];
  timeout?: number;
  excludeCredentials?: Descriptor[];
  authenticatorSelection?: AuthenticatorSelectionCriteria;
  attestation?: AttestationConveyancePreference;
}
export interface RequestOptionsJSON {
  rpId: string;
  challenge: string;
  timeout?: number;
  allowCredentials?: Descriptor[];
  userVerification?: UserVerificationRequirement;
}

const descriptors = (list: Descriptor[] | undefined): PublicKeyCredentialDescriptor[] =>
  (list ?? []).map((d) => ({ ...d, id: b64urlToBytes(d.id) }));

export function decodeCreationOptions(o: CreationOptionsJSON): PublicKeyCredentialCreationOptions {
  return {
    ...o,
    challenge: b64urlToBytes(o.challenge),
    user: { ...o.user, id: b64urlToBytes(o.user.id) },
    excludeCredentials: descriptors(o.excludeCredentials),
  };
}

export function decodeRequestOptions(o: RequestOptionsJSON): PublicKeyCredentialRequestOptions {
  return {
    ...o,
    challenge: b64urlToBytes(o.challenge),
    allowCredentials: descriptors(o.allowCredentials),
  };
}

export interface RegistrationCredentialJSON {
  rawId: string;
  response: { clientDataJSON: string; attestationObject: string };
}
export interface AssertionCredentialJSON {
  rawId: string;
  response: {
    clientDataJSON: string;
    authenticatorData: string;
    signature: string;
    userHandle: string | null;
  };
}

/** `navigator.credentials.create` for the options of `register/begin`. */
export async function createPasskey(
  options: CreationOptionsJSON,
): Promise<RegistrationCredentialJSON> {
  const cred = (await navigator.credentials.create({
    publicKey: decodeCreationOptions(options),
  })) as PublicKeyCredential | null;
  if (!cred) throw new DOMException('No credential returned', 'NotAllowedError');
  const res = cred.response as AuthenticatorAttestationResponse;
  return {
    rawId: bytesToB64url(cred.rawId),
    response: {
      clientDataJSON: bytesToB64url(res.clientDataJSON),
      attestationObject: bytesToB64url(res.attestationObject),
    },
  };
}

/** `navigator.credentials.get` for the options of `login/begin` or `verify/begin`. */
export async function getPasskey(options: RequestOptionsJSON): Promise<AssertionCredentialJSON> {
  const cred = (await navigator.credentials.get({
    publicKey: decodeRequestOptions(options),
  })) as PublicKeyCredential | null;
  if (!cred) throw new DOMException('No credential returned', 'NotAllowedError');
  const res = cred.response as AuthenticatorAssertionResponse;
  return {
    rawId: bytesToB64url(cred.rawId),
    response: {
      clientDataJSON: bytesToB64url(res.clientDataJSON),
      authenticatorData: bytesToB64url(res.authenticatorData),
      signature: bytesToB64url(res.signature),
      userHandle: res.userHandle ? bytesToB64url(res.userHandle) : null,
    },
  };
}

/** The user dismissed or timed out the browser prompt. */
export const isCancelled = (e: unknown): boolean =>
  e instanceof DOMException && (e.name === 'NotAllowedError' || e.name === 'AbortError');

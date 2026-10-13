import {
  CREATION_OPTIONS,
  REQUEST_OPTIONS,
  stubWebAuthn,
  unstubWebAuthn,
} from '../test/webauthnFake';
import {
  b64urlToBytes,
  bytesToB64url,
  createPasskey,
  decodeCreationOptions,
  decodeRequestOptions,
  getPasskey,
  isCancelled,
  passkeysSupported,
} from './webauthn';

afterEach(unstubWebAuthn);

describe('base64url', () => {
  it('round-trips bytes including ones that need - and _', () => {
    const bytes = new Uint8Array([0xfb, 0xff, 0xfe, 0x00, 0x3e, 0x3f]);
    const s = bytesToB64url(bytes);
    expect(s).toBe('-__-AD4_');
    expect(Array.from(b64urlToBytes(s))).toEqual(Array.from(bytes));
    expect(bytesToB64url(new Uint8Array([1]))).toBe('AQ');
    expect(Array.from(b64urlToBytes('AQ'))).toEqual([1]);
  });
});

describe('options decoding', () => {
  it('turns the base64url fields of creation options into bytes', () => {
    const o = decodeCreationOptions(CREATION_OPTIONS as never);
    expect(Array.from(new Uint8Array(o.user.id as ArrayBuffer))).toEqual([1, 2, 3]);
    expect(o.challenge).toBeInstanceOf(Uint8Array);
    expect(Array.from(new Uint8Array(o.excludeCredentials?.[0]?.id as ArrayBuffer))).toEqual([
      4, 5,
    ]);
    expect(o.rp.id).toBe('localhost');
  });

  it('keeps an empty allow list (the discoverable flow)', () => {
    const o = decodeRequestOptions(REQUEST_OPTIONS as never);
    expect(o.allowCredentials).toEqual([]);
    expect(o.rpId).toBe('localhost');
  });
});

describe('ceremonies', () => {
  it('detects support', () => {
    expect(passkeysSupported()).toBe(false);
    stubWebAuthn();
    expect(passkeysSupported()).toBe(true);
  });

  it('encodes a registration result', async () => {
    const { create } = stubWebAuthn();
    const out = await createPasskey(CREATION_OPTIONS as never);
    expect(create).toHaveBeenCalledOnce();
    expect(out).toEqual({
      rawId: 'AQID',
      response: { clientDataJSON: 'BAU', attestationObject: 'BgcI' },
    });
  });

  it('encodes an assertion result with the user handle', async () => {
    stubWebAuthn();
    const out = await getPasskey(REQUEST_OPTIONS as never);
    expect(out).toEqual({
      rawId: 'AQID',
      response: {
        clientDataJSON: 'CQ',
        authenticatorData: 'Cgs',
        signature: 'DA',
        userHandle: 'DQ4',
      },
    });
  });

  it('classifies a dismissed prompt as cancelled', async () => {
    stubWebAuthn({ reject: new DOMException('x', 'NotAllowedError') });
    const e = await getPasskey(REQUEST_OPTIONS as never).catch((err: unknown) => err);
    expect(isCancelled(e)).toBe(true);
    expect(isCancelled(new Error('boom'))).toBe(false);
  });
});

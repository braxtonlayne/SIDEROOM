import { OutlineButton } from '@sideroom/ui';
import { useState } from 'react';
import { passkeyOptions, passkeyVerify } from '../api/entrance';

/** WebAuthn JSON helpers are newer than the TS DOM lib, so they are typed locally. */
interface WebAuthnStatics {
  parseRequestOptionsFromJSON?: (o: unknown) => unknown;
}
export const passkeysSupported = () =>
  typeof window !== 'undefined' && 'PublicKeyCredential' in window && !!navigator.credentials;

/** ENT-UI-016: only rendered when the browser can do WebAuthn. */
export function PasskeyButton({
  onDone,
  onError,
  label = 'Use a passkey',
}: {
  onDone: () => void;
  onError: (message: string) => void;
  label?: string;
}) {
  const [busy, setBusy] = useState(false);
  if (!passkeysSupported()) return null;

  const run = async () => {
    setBusy(true);
    try {
      const options = await passkeyOptions();
      const statics = PublicKeyCredential as unknown as WebAuthnStatics;
      const publicKey = (statics.parseRequestOptionsFromJSON?.(options) ??
        options) as PublicKeyCredentialRequestOptions;
      const cred = (await navigator.credentials.get({ publicKey })) as
        (Credential & { toJSON?: () => unknown }) | null;
      if (!cred) throw new Error('cancelled');
      await passkeyVerify(cred.toJSON?.() ?? cred);
      onDone();
    } catch (e) {
      const cancelled = e instanceof DOMException && e.name === 'NotAllowedError';
      onError(
        cancelled
          ? 'Passkey sign-in was cancelled.'
          : "That passkey didn't work. Try another way in.",
      );
      setBusy(false);
    }
  };

  return (
    <OutlineButton type="button" onClick={() => void run()} disabled={busy} aria-busy={busy}>
      {label}
    </OutlineButton>
  );
}

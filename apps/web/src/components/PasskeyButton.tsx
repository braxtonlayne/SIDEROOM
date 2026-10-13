import { OutlineButton } from '@sideroom/ui';
import { useState } from 'react';
import { ApiError } from '../api/client';
import { submitTwoFactor } from '../api/entrance';
import { passkeyLoginBegin, passkeyLoginFinish } from '../api/passkeys';
import { getPasskey, isCancelled, passkeysSupported } from '../lib/webauthn';

export { passkeysSupported };

/**
 * ENT-UI-016: "Use a passkey". Only rendered when the browser can do WebAuthn.
 *
 * Without `secondFactor` it is passkey-only sign-in (discoverable credential, user verification
 * required). With it, it answers the second step of a password login: the ceremony is begun for
 * the account waiting on the `sr_mfa` cookie and finished through `POST /sr/auth/2fa`.
 */
export function PasskeyButton({
  onDone,
  onError,
  label = 'Use a passkey',
  secondFactor,
}: {
  onDone: () => void;
  onError: (message: string) => void;
  label?: string;
  secondFactor?: { trustDevice: boolean };
}) {
  const [busy, setBusy] = useState(false);
  if (!passkeysSupported()) return null;

  const run = async () => {
    setBusy(true);
    try {
      const { ceremony_id, options } = await passkeyLoginBegin(!!secondFactor);
      const credential = await getPasskey(options);
      if (secondFactor) {
        await submitTwoFactor({
          method: 'passkey',
          code: JSON.stringify({ ceremony_id, credential }),
          trust_device: secondFactor.trustDevice,
        });
      } else {
        await passkeyLoginFinish({ ceremony_id, credential });
      }
      onDone();
    } catch (e) {
      if (e instanceof ApiError && e.status === 429) {
        onError('Too many attempts. Wait a moment and try again.');
      } else if (isCancelled(e)) {
        onError('Passkey sign-in was cancelled.');
      } else {
        onError("That passkey didn't work. Try another way in.");
      }
      setBusy(false);
    }
  };

  return (
    <OutlineButton type="button" onClick={() => void run()} disabled={busy} aria-busy={busy}>
      {label}
    </OutlineButton>
  );
}

// Passkey endpoints (crates/sr-api/src/routes/auth/passkeys.rs, docs/05 section 5.1).
import type {
  AssertionCredentialJSON,
  CreationOptionsJSON,
  RegistrationCredentialJSON,
  RequestOptionsJSON,
} from '../lib/webauthn';
import { apiFetch, apiPost, refreshCsrf } from './client';

export interface Passkey {
  id: string;
  name: string;
  created_at: string;
  last_used_at: string | null;
  backup_eligible: boolean;
}
interface Begin<O> {
  ceremony_id: string;
  options: O;
}

export const listPasskeys = () => apiFetch<Passkey[]>('/sr/auth/passkeys', { retries: 0 });
/** Re-checks the password (403 with a message when it is wrong), then returns creation options. */
export const passkeyRegisterBegin = (password: string) =>
  apiPost<Begin<CreationOptionsJSON>>('/sr/auth/passkeys/register/begin', { password });
export const passkeyRegisterFinish = (req: {
  ceremony_id: string;
  name?: string;
  credential: RegistrationCredentialJSON;
}) =>
  apiPost<{ passkey: Passkey; recovery_codes: string[] | null }>(
    '/sr/auth/passkeys/register/finish',
    req,
  );
export const renamePasskey = (id: string, name: string) =>
  apiFetch<Passkey>(`/sr/auth/passkeys/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    json: { name },
    retries: 0,
  });
export const deletePasskey = (id: string, password: string) =>
  apiFetch<void>(`/sr/auth/passkeys/${encodeURIComponent(id)}`, {
    method: 'DELETE',
    json: { password },
    retries: 0,
  });

/** Passkey-only login (discoverable credential). */
export const passkeyLoginBegin = (secondFactor = false) =>
  apiPost<Begin<RequestOptionsJSON>>('/sr/auth/passkeys/login/begin', {
    second_factor: secondFactor,
  });
export const passkeyLoginFinish = async (req: {
  ceremony_id: string;
  credential: AssertionCredentialJSON;
}) => {
  const res = await apiPost<{ status: 'ok' }>('/sr/auth/passkeys/login/finish', req);
  await refreshCsrf();
  return res;
};

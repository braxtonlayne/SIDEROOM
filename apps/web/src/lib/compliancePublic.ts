// What visitors may know about the instance's compliance settings (M-06, D-293 to D-295):
// `GET /sr/compliance/public` answers the minimum registration age and, when the operator set
// them, the country and the contact address. Public, so the entrance pages can read it before
// sign-in. First-paint core: no t() keys and no heavy imports. An old or unreachable server keeps
// the defaults (13, nothing else).
import { useEffect, useState } from 'react';
import { apiFetch } from '../api/client';

export interface CompliancePublic {
  minimum_age: number;
  /** Years invoices stay linked to a member after the year of payment (D-325). */
  invoice_retention_years?: number;
  /** Invite credits a new account is granted (`invites.default_credits`). */
  invite_initial_credits?: number;
  /** The operator locked the doors (`invites.locked`): no registration succeeds, the entrance says so. */
  doors_closed?: boolean;
  operator_country?: string;
  operator_contact?: string;
}

export const DEFAULT_COMPLIANCE: CompliancePublic = { minimum_age: 13 };

let cache: CompliancePublic | null = null;
let inflight: Promise<CompliancePublic> | null = null;

function load(): Promise<CompliancePublic> {
  inflight ??= apiFetch<CompliancePublic>('/sr/compliance/public', { retries: 0 })
    .then((c) => {
      cache =
        c && typeof c.minimum_age === 'number' && c.minimum_age >= 13 && c.minimum_age <= 18
          ? c
          : DEFAULT_COMPLIANCE;
      return cache;
    })
    .catch(() => DEFAULT_COMPLIANCE)
    .finally(() => {
      inflight = null;
    });
  return inflight;
}

/** Drops the cached answer (tests). */
export function resetCompliancePublic() {
  cache = null;
  inflight = null;
}

/** The public compliance settings: the defaults until the answer arrives. */
export function useCompliancePublic(): CompliancePublic {
  const [value, setValue] = useState<CompliancePublic>(cache ?? DEFAULT_COMPLIANCE);
  useEffect(() => {
    let live = true;
    if (cache) setValue(cache);
    else
      void load().then((c) => {
        if (live) setValue(c);
      });
    return () => {
      live = false;
    };
  }, []);
  return value;
}

// Test helper (STU-NAV-001): the storefront surfaces are off by default, so the tests that exercise them switch them on.
import { setClientFlags } from '../moderation/reports';
import { STUDY_FLAGS } from './flags';

/** Switch every Study storefront surface on (the operator opted in). Pass `off` to leave some of them off. */
export function switchStudySurfacesOn(off: string[] = []): void {
  const flags: Record<string, boolean> = {};
  for (const id of Object.values(STUDY_FLAGS)) flags[id] = !off.includes(id);
  setClientFlags(flags);
}

/** Back to the shipped default: no flag known, so every surface is hidden. */
export function resetStudySurfaces(): void {
  setClientFlags({});
}

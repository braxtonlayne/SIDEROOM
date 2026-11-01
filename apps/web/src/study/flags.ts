// STU-NAV-001 (wave 41, The Study): the storefront surfaces are switched off by default and hidden, never deleted.
// The switches are client-visible platform flags (`GET /sr/features/flags`). Unlike the kill switches of the tools,
// a flag that is ABSENT counts as off here: a surface appears only when the operator switched it on.
import { useClientFlag } from '../moderation/reports';

export const STUDY_FLAGS = {
  externalMetadata: 'platform.external_metadata',
  storefrontCategories: 'platform.storefront_categories',
  globalTrending: 'platform.global_trending',
  requestsBoard: 'platform.requests_board',
  ingestQueues: 'platform.ingest_queues',
} as const;

export type StudyFlag = keyof typeof STUDY_FLAGS;

/** Whether the storefront surface is switched on (reactive). */
export function useStudyFlag(flag: StudyFlag): boolean {
  return useClientFlag(STUDY_FLAGS[flag]);
}

/** Pure form for tests and non-React code. */
export function studyFlagOn(flags: Record<string, boolean>, flag: StudyFlag): boolean {
  return flags[STUDY_FLAGS[flag]] === true;
}

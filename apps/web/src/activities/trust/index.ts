// The small interface the Arcade page (ARC-ROOM-001) renders build trust through (ARC-SBX-001).
//
//   const state = useBuildTrust(applicationId);   // loads GET /activities/{id}/trust, vouch/unvouch actions
//   <BuildTrust state={state} />                   // badges + "Vouch for Build"
//   <BuildBadges trust={view} />                   // badges only, for a card when you already hold a TrustView
//
// `badgeModels(view)` / `badgeTag(kind)` give the same facts without React.
export { BuildBadges, BuildTrust, VouchControl } from './BuildBadge';
export { badgeModel, badgeModels, badgeTag, vouchState } from './badge';
export type { BadgeModel, BadgeTone, VouchState } from './badge';
export { httpTrustApi, type TrustApi } from './api';
export { useBuildTrust, type BuildTrustState } from './useBuildTrust';
export type { BadgeKind, PreviousBuild, SignatureView, TrustView } from './types';

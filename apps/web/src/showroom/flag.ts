// UX-SHOW-001 (wave 45-B): the Showroom (populated preview) is a beta/test aid. The deployment switch below is a
// client-visible platform flag (`GET /sr/features/flags`), OFF by default; an absent flag counts as off. It gates only
// this preview. It never touches end-to-end encryption, per-route authorization or the audit and access logs.
export const SHOWROOM_FLAG = 'platform.showroom';

/** The per-device choice (localStorage). Only meaningful while the deployment switch is on. */
export const SHOWROOM_STORAGE_KEY = 'sr.showroom.v1';

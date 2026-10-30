import { apiPost } from './client';

/**
 * The ban screen: the credentials prove the (session-less) banned account. `POST /moderation/appeals`. It lives in
 * its own module because only the auth surface may carry it (a banned member cannot sign in, so the appeal stays
 * public, D-385); the landing and invite surfaces must not.
 */
export const fileAppeal = (body: { login: string; password: string; message: string }) =>
  apiPost<void>('/moderation/appeals', body);

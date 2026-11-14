import { apiFetch, ApiError } from '../api/client';
import type {
  BugReportBody,
  BugReportCreated,
  BugReportDetail,
  BugReportList,
  BugStatus,
} from './contract';

export const sendBugReport = (body: BugReportBody): Promise<BugReportCreated> =>
  apiFetch<BugReportCreated>('/bug-reports', { method: 'POST', json: body, retries: 0 });

/** `before` is the id of the last report already shown: the page that follows it (SEC-AUD-267). */
export const listBugReports = (
  status: BugStatus = 'open',
  limit = 50,
  before?: string,
): Promise<BugReportList> =>
  apiFetch<BugReportList>(
    `/admin/bug-reports?status=${status}&limit=${limit}${
      before ? `&before=${encodeURIComponent(before)}` : ''
    }`,
  );

export const getBugReport = (id: string): Promise<BugReportDetail> =>
  apiFetch<BugReportDetail>(`/admin/bug-reports/${encodeURIComponent(id)}`);

export const closeBugReport = (id: string): Promise<void> =>
  apiFetch<void>(`/admin/bug-reports/${encodeURIComponent(id)}/close`, {
    method: 'POST',
    json: {},
    retries: 0,
  });

/** The picture's bytes (`apiFetch` parses JSON, so this reads the response itself). Call it only on the staff member's click. */
export async function getBugScreenshot(id: string): Promise<Blob> {
  const res = await fetch(`/api/v1/admin/bug-reports/${encodeURIComponent(id)}/screenshot`, {
    credentials: 'include',
  });
  if (!res.ok) throw new ApiError(res.status, res.statusText, null);
  return res.blob();
}

// SEC-AUD-165 (D-647): the beta bug reporter is a client-visible platform switch, OFF by default; an absent flag counts
// as off. It gates only the reporter and never touches end-to-end encryption, authorization or the audit log.
export const BETA_REPORTS_FLAG = 'platform.beta_reports';

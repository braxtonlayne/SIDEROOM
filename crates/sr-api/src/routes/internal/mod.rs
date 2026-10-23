//! Routes that browsers and proxies call without a session: `POST /internal/csp-report` (RDY-SEC-009).
//!
//! The Content-Security-Policy of the app shell names this path in `report-uri` and `report-to`. Browsers post
//! one violation per legacy report (`application/csp-report`) or a list in the Reporting API format
//! (`application/reports+json`). The route needs no credentials, so it is rate limited per address and in total,
//! reads at most [`MAX_BODY`] bytes, and stores only counters: the violated directive, the blocked origin and
//! the document path. It never keeps the query, the fragment, the referrer, the user agent, the source file, the
//! script sample or the reporter's address.

mod csp;

pub use csp::{
    CspCounter, CspReportStore, CspReports, CspSnapshot, MAX_BODY, MemoryCspReports, Violation,
    csp_report, parse_reports,
};

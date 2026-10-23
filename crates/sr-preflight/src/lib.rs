//! Preflight: is this box ready for its first invite? (RDY-OPS-004)
//!
//! One list of checks, run by `sr-cli preflight` and by `POST /admin/system/preflight`. Every check is a
//! function over small probe traits ([`DbProbe`], [`NetProbe`], [`HostProbe`] and the storage trait), so each
//! has unit tests with fakes; [`probes`] holds the real implementations. A check never panics and never
//! returns an error: a failure is a row with [`Status::Fail`] and a sentence that says what to do.
//!
//! Dependency rule (D-358): this crate is an operator-fixed-endpoint exception to the `reqwest` wrapper list,
//! like `sr-push`, `sr-storage` and `sr-transcoder`. `reqwest` is used only in `probes::RealNet::http_get`, against
//! `server.public_url` and `SR_PAYMENTS_ESPLORA_URL`; the first check that takes an address which is not operator
//! configuration must go through `sr_net::SafeClient` instead.

mod checks;
mod md5;
pub mod probes;
mod report;
mod settings;
pub mod turn;

pub use checks::{
    Ctx, DbProbe, HostProbe, HttpReply, MigrationState, ModeResult, NetProbe, database_checks, run,
};
pub use report::{CHECK_IDS, CLI_EXTRA_IDS, Check, Report, Status};
pub use settings::{Settings, SmtpTarget};

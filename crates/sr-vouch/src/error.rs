use thiserror::Error;

pub type VouchResult<T> = Result<T, VouchError>;

#[derive(Debug, Error)]
pub enum VouchError {
    /// A member cannot vouch for themselves.
    #[error("you cannot vouch for yourself")]
    SelfVouch,
    /// The target does not exist, is not an active member, or either side blocked the other. One answer for all
    /// three, so the call is no oracle for blocks.
    #[error("no such member")]
    NotFound,
    /// The caller may not vouch: not in good standing, or sterilized (wave 44-A).
    #[error("you cannot vouch right now")]
    NotAllowed,
    /// The caller vouched for too many members today, or holds the most vouches a member can give.
    #[error("{0}")]
    RateLimited(&'static str),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
}

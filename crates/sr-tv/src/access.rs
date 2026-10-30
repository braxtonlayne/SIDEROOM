//! Who may watch a server's channel (BYOS-TV-001): the port the composition root fills from the server's
//! storage grants. Instance channels and personal channels are decided inside [`crate::TvService`].

use async_trait::async_trait;

use crate::error::TvResult;
use crate::model::PoolSource;
use crate::service::Viewer;

#[async_trait]
pub trait ChannelAccess: Send + Sync {
    /// Whether `viewer` may watch a channel with this (server) pool: a member of the server who holds the
    /// `view_cinema` grant. A failing store answers an error, never `true`.
    async fn may_watch(&self, viewer: Viewer, pool: PoolSource) -> TvResult<bool>;
}

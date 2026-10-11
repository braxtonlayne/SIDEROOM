//! Collaborators of the social endpoints. Registered on the composition root as an
//! [`AppState`](crate::AppState) extension and read with [`crate::routes::Ext`].

use super::dynrepo::DynRepo;
use super::events::{SocialEvent, publish_social};
use super::model::UserObject;
use crate::routes::messages::error::MsgResult;
use sr_chat::repo::{UserLookup, UserSummary};
use sr_chat::social::{SocialLimits, SocialService};
use sr_core::snowflake::SnowflakeError;
use sr_core::{Clock, SnowflakeGenerator, UserId};
use sr_events::EventBus;
use std::collections::HashMap;
use std::sync::Arc;

pub struct SocialDeps {
    pub service: SocialService<DynRepo>,
    users: Arc<dyn UserLookup>,
    bus: Arc<dyn EventBus>,
    clock: Arc<dyn Clock>,
}

impl SocialDeps {
    /// `worker`/`process` identify this node's snowflake generator for DM channel
    /// ids; pick a process id no other generator on the node uses.
    pub fn new(
        repo: DynRepo,
        users: Arc<dyn UserLookup>,
        bus: Arc<dyn EventBus>,
        clock: Arc<dyn Clock>,
        limits: SocialLimits,
        worker: u8,
        process: u8,
    ) -> Result<Self, SnowflakeError> {
        let ids = SnowflakeGenerator::new(clock.clone(), worker, process)?;
        Ok(Self {
            service: SocialService::new(repo, clock.clone(), ids, limits),
            users,
            bus,
            clock,
        })
    }

    pub fn now_ms(&self) -> i64 {
        self.clock.now_unix_ms()
    }

    /// Display fields for `ids`; unknown users are absent.
    pub async fn summaries(
        &self,
        ids: impl IntoIterator<Item = UserId>,
    ) -> MsgResult<HashMap<UserId, UserSummary>> {
        let mut ids: Vec<UserId> = ids.into_iter().collect();
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let found = self.users.users(&ids).await?;
        Ok(found.into_iter().map(|u| (u.id, u)).collect())
    }

    pub async fn user_object(&self, id: UserId) -> MsgResult<UserObject> {
        let map = self.summaries([id]).await?;
        Ok(UserObject::new(id, &map))
    }

    /// Best-effort delivery; see [`publish_social`].
    pub async fn emit(&self, ev: SocialEvent) {
        publish_social(self.bus.as_ref(), ev).await;
    }
}

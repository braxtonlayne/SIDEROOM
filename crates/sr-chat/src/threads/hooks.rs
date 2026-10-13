//! Fan a stored message out to several [`MessageHooks`].

use crate::domain::Message;
use crate::message_hooks::MessageHooks;
use async_trait::async_trait;
use sr_core::UserId;
use std::sync::Arc;

/// Runs each hook in order; one hook's failure handling is its own business
/// (hooks log instead of returning errors).
pub struct HookChain(pub Vec<Arc<dyn MessageHooks>>);

#[async_trait]
impl MessageHooks for HookChain {
    async fn message_sent(&self, message: &Message, notify: &[UserId]) {
        for h in &self.0 {
            h.message_sent(message, notify).await;
        }
    }
}

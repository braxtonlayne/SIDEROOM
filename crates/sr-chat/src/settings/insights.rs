//! Server Insights (LNG-SSET-068): daily counters per guild, computed by a job in
//! `sr-server` and read through [`super::repo::InsightsRepository`].

use serde::{Deserialize, Serialize};
use sr_core::ChannelId;

pub const DEFAULT_DAYS: u32 = 30;
pub const MAX_DAYS: u32 = 90;
pub const TOP_CHANNELS: usize = 10;

/// One UTC day.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InsightsDay {
    /// `YYYY-MM-DD`.
    pub day: String,
    /// Members at the end of the day.
    pub members: i64,
    pub joins: i64,
    pub messages: i64,
    pub voice_minutes: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopChannel {
    pub channel_id: ChannelId,
    pub messages: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Insights {
    /// Oldest first; days the job has not computed are absent.
    pub days: Vec<InsightsDay>,
    /// Busiest channels over the window, most messages first.
    pub top_channels: Vec<TopChannel>,
}

/// Clamp a requested window to `1..=MAX_DAYS`, defaulting to 30.
pub fn clamp_days(days: Option<u32>) -> u32 {
    days.unwrap_or(DEFAULT_DAYS).clamp(1, MAX_DAYS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_is_clamped() {
        assert_eq!(clamp_days(None), 30);
        assert_eq!(clamp_days(Some(0)), 1);
        assert_eq!(clamp_days(Some(7)), 7);
        assert_eq!(clamp_days(Some(1000)), 90);
    }
}

//! Video on the voice WebSocket (VOX-SFU-005, 006, 008): publishing camera and screen
//! tracks, subscriber hints and the egress governor's notice.
//!
//! * Op 12 `VIDEO`, both directions, one JSON object with a `t` tag ([`Video`]).
//! * Op 13 `QUALITY`, server to client ([`Quality`]).
//!
//! # Tracks and stream groups
//!
//! A participant publishes up to three tracks, each on its own `sendonly` m-line of its
//! offer: the **camera** (simulcast, three layers `q`/`h`/`f`), the **screen** and the
//! **screen audio**. The client names which m-line is which with
//! `{"t":"publish","mid":"3","source":"screen",...}` (either before or after the offer that
//! carries the line). The SFU forwards the screen and its audio to a subscriber on two
//! `sendonly` lines that share the msid stream id `"<user>:screen"`: the browser plays
//! them as one synchronized stream group. The microphone is stream id `"<user>"` and the
//! camera `"<user>:camera"`.
//!
//! # What a subscriber receives
//!
//! * Cameras are on by default, at the smallest layer until the client says otherwise.
//! * Screens (and their audio) are off until the client subscribes; the gateway tells the
//!   SFU who may watch ([`crate::VoiceCommand::StreamViewers`]), so a client cannot
//!   subscribe around the viewer cap.
//! * A [`SubscriptionHint`] names a layer or a viewport; the SFU picks the smallest layer
//!   that covers the viewport, then lowers it to fit the bandwidth estimate and the
//!   governor's cap. Switching layers waits for a keyframe.

use serde::{Deserialize, Serialize};
use sr_core::UserId;

/// What a published track carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Camera,
    Screen,
    ScreenAudio,
}

impl Source {
    /// The msid stream id suffix that groups a source's lines for the subscriber.
    pub const fn group(self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::Screen | Self::ScreenAudio => "screen",
        }
    }

    pub const fn is_video(self) -> bool {
        matches!(self, Self::Camera | Self::Screen)
    }
}

/// A simulcast layer, smallest first. The RIDs are Chrome's convention.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    /// A quarter of the resolution, the thumbnail.
    #[default]
    Low,
    /// Half.
    Mid,
    /// Full.
    High,
}

impl Layer {
    pub const ALL: [Layer; 3] = [Self::Low, Self::Mid, Self::High];

    pub const fn rid(self) -> &'static str {
        match self {
            Self::Low => "q",
            Self::Mid => "h",
            Self::High => "f",
        }
    }

    pub fn from_rid(rid: &str) -> Option<Self> {
        match rid {
            "q" => Some(Self::Low),
            "h" => Some(Self::Mid),
            "f" => Some(Self::High),
            _ => None,
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Self::Low => 0,
            Self::Mid => 1,
            Self::High => 2,
        }
    }

    /// The next smaller layer, if any.
    pub const fn lower(self) -> Option<Self> {
        match self {
            Self::Low => None,
            Self::Mid => Some(Self::Low),
            Self::High => Some(Self::Mid),
        }
    }

    /// The next larger layer, if any.
    pub const fn higher(self) -> Option<Self> {
        match self {
            Self::Low => Some(Self::Mid),
            Self::Mid => Some(Self::High),
            Self::High => None,
        }
    }
}

/// Size of the tile a video is shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewport {
    pub width: u32,
    pub height: u32,
}

/// What a subscriber wants from one source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionHint {
    pub user_id: UserId,
    pub source: Source,
    /// `false` turns the source off for this subscriber.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// The layer the client would like at most; wins over the viewport.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<Layer>,
    /// Where the video is drawn; the SFU picks the smallest layer that covers it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewport: Option<Viewport>,
}

fn yes() -> bool {
    true
}

/// Op 12.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Video {
    /// C->S: the m-line `mid` (the client's own mid) carries `source`. `active: false`
    /// announces the end of the track (the line is stopped by the next offer).
    Publish {
        mid: String,
        source: Source,
        #[serde(default = "yes")]
        active: bool,
        #[serde(default)]
        width: u32,
        #[serde(default)]
        height: u32,
        #[serde(default)]
        fps: u32,
    },
    /// C->S: replace the subscriber hints. Sources not named keep their defaults.
    Subscribe {
        subscriptions: Vec<SubscriptionHint>,
    },
    /// S->C: a participant started or stopped publishing a source.
    Published {
        user_id: UserId,
        source: Source,
        active: bool,
        #[serde(default)]
        width: u32,
        #[serde(default)]
        height: u32,
        #[serde(default)]
        fps: u32,
    },
    /// S->C: the layer now sent for `user_id`'s `source` changed.
    Layer {
        user_id: UserId,
        source: Source,
        layer: Layer,
        reason: LayerReason,
    },
}

/// Why the SFU changed a layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerReason {
    /// The client asked (a hint changed).
    Hint,
    /// The bandwidth estimate to this client changed.
    Bandwidth,
    /// The egress governor capped it.
    Governor,
}

/// Op 13, S->C: the egress governor's notice. "Reduced quality: the room is busy."
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quality {
    /// `true` while this subscriber's layers are capped by the governor.
    pub reduced: bool,
    /// The largest layer the governor allows this subscriber now (`high` when not reduced).
    pub max_layer: Layer,
    pub reason: QualityReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityReason {
    /// The node is near its egress budget.
    RoomBusy,
    /// The cap was lifted.
    Normal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn layers_map_to_rids_and_order_by_size() {
        for l in Layer::ALL {
            assert_eq!(Layer::from_rid(l.rid()), Some(l));
        }
        assert_eq!(Layer::from_rid("x"), None);
        assert!(Layer::Low < Layer::Mid && Layer::Mid < Layer::High);
        assert_eq!(Layer::High.lower(), Some(Layer::Mid));
        assert_eq!(Layer::Low.lower(), None);
        assert_eq!(Layer::Low.higher(), Some(Layer::Mid));
        assert_eq!(Layer::High.higher(), None);
        assert_eq!(Layer::Mid.index(), 1);
    }

    #[test]
    fn screen_and_its_audio_share_a_stream_group() {
        assert_eq!(Source::Screen.group(), Source::ScreenAudio.group());
        assert_ne!(Source::Camera.group(), Source::Screen.group());
        assert!(Source::Camera.is_video() && Source::Screen.is_video());
        assert!(!Source::ScreenAudio.is_video());
    }

    #[test]
    fn publish_and_subscribe_round_trip() {
        let m: Video = serde_json::from_value(json!({
            "t": "publish", "mid": "3", "source": "screen_audio"
        }))
        .unwrap();
        assert_eq!(
            m,
            Video::Publish {
                mid: "3".into(),
                source: Source::ScreenAudio,
                active: true,
                width: 0,
                height: 0,
                fps: 0
            }
        );
        let s: Video = serde_json::from_value(json!({
            "t": "subscribe",
            "subscriptions": [
                {"user_id": "7", "source": "camera", "viewport": {"width": 320, "height": 180}},
                {"user_id": "7", "source": "screen", "enabled": false}
            ]
        }))
        .unwrap();
        let Video::Subscribe { subscriptions } = &s else {
            panic!("{s:?}")
        };
        assert!(subscriptions[0].enabled && !subscriptions[1].enabled);
        assert_eq!(subscriptions[0].viewport.unwrap().width, 320);
        assert_eq!(subscriptions[0].layer, None);
        let text = serde_json::to_value(&s).unwrap();
        assert_eq!(text["t"], "subscribe");
        assert_eq!(text["subscriptions"][0]["user_id"], "7");
    }

    #[test]
    fn server_messages_have_the_documented_shape() {
        let p = serde_json::to_value(Video::Published {
            user_id: UserId::new(5),
            source: Source::Camera,
            active: true,
            width: 1280,
            height: 720,
            fps: 30,
        })
        .unwrap();
        assert_eq!(
            p,
            json!({"t": "published", "user_id": "5", "source": "camera", "active": true,
                   "width": 1280, "height": 720, "fps": 30})
        );
        let l = serde_json::to_value(Video::Layer {
            user_id: UserId::new(5),
            source: Source::Camera,
            layer: Layer::Mid,
            reason: LayerReason::Governor,
        })
        .unwrap();
        assert_eq!(l["layer"], "mid");
        assert_eq!(l["reason"], "governor");
        let q = serde_json::to_value(Quality {
            reduced: true,
            max_layer: Layer::Mid,
            reason: QualityReason::RoomBusy,
        })
        .unwrap();
        assert_eq!(
            q,
            json!({"reduced": true, "max_layer": "mid", "reason": "room_busy"})
        );
    }

    #[test]
    fn unknown_tags_and_sources_are_rejected() {
        assert!(serde_json::from_value::<Video>(json!({"t": "nope"})).is_err());
        assert!(
            serde_json::from_value::<Video>(json!({"t": "publish", "mid": "1", "source": "x"}))
                .is_err()
        );
    }
}

//! Where a channel's pools draw from (BYOS-TV-001, migration 1510).

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::model::id;

/// Where a channel's pools draw from.
///
/// * `instance`: the instance catalog, as before (no owner).
/// * `guild`: a server's shelves only, that is the server's own mounts and the instance bucket (mount 1) when
///   it is attached to the server. Every viewer who may watch the server's Cinema sees the same guide, so
///   nothing is skipped per viewer.
/// * `personal`: one member's own shelf, for that member alone. A friend's shelf is never a source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[schema(as = TvPoolSource)]
pub enum PoolSource {
    #[default]
    Instance,
    Guild {
        #[serde(with = "id")]
        #[schema(value_type = String)]
        guild_id: i64,
    },
    Personal {
        #[serde(with = "id")]
        #[schema(value_type = String)]
        user_id: i64,
    },
}

impl PoolSource {
    /// The stored name (`tv_channels.pool_source`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Instance => "instance",
            Self::Guild { .. } => "guild",
            Self::Personal { .. } => "personal",
        }
    }

    /// The server id, for a server channel.
    pub fn guild_id(self) -> Option<i64> {
        match self {
            Self::Guild { guild_id } => Some(guild_id),
            _ => None,
        }
    }

    /// The owner, for a personal channel.
    pub fn owner_id(self) -> Option<i64> {
        match self {
            Self::Personal { user_id } => Some(user_id),
            _ => None,
        }
    }

    /// The guide order of sources (P29-TV-1): instance first, then servers by id, then members by id; channel
    /// numbers repeat across sources, so the number sorts within this key. Mirrors the `ORDER BY` of the
    /// Postgres repository.
    pub fn guide_rank(self) -> (u8, i64, i64) {
        match self {
            Self::Instance => (0, 0, 0),
            Self::Guild { guild_id } => (1, guild_id, 0),
            Self::Personal { user_id } => (2, 0, user_id),
        }
    }

    /// Rebuild from the stored columns; `None` when they disagree.
    pub fn from_columns(name: &str, guild_id: Option<i64>, owner: Option<i64>) -> Option<Self> {
        match (name, guild_id, owner) {
            ("instance", None, None) => Some(Self::Instance),
            ("guild", Some(guild_id), None) => Some(Self::Guild { guild_id }),
            ("personal", None, Some(user_id)) => Some(Self::Personal { user_id }),
            _ => None,
        }
    }

    /// The text a refusal gives when an item is not on a shelf this source may draw from.
    pub fn refusal(self) -> &'static str {
        match self {
            Self::Instance => "That title is not part of the instance catalog",
            Self::Guild { .. } => {
                "That title is not on this server's shelves. A server channel airs only the \
                 server's own shelves and the instance bucket when it is attached, never a \
                 member's personal shelf"
            }
            Self::Personal { .. } => "A personal channel airs only your own shelf",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guide_lists_instance_then_servers_then_personal_by_number_within_each() {
        let a = PoolSource::Guild { guild_id: 10 };
        let b = PoolSource::Guild { guild_id: 20 };
        let me = PoolSource::Personal { user_id: 5 };
        let mut v = vec![
            (me, 1),
            (b, 1),
            (a, 2),
            (PoolSource::Instance, 7),
            (a, 1),
            (PoolSource::Instance, 1),
        ];
        v.sort_by_key(|(p, n)| (p.guide_rank(), *n));
        assert_eq!(
            v,
            vec![
                (PoolSource::Instance, 1),
                (PoolSource::Instance, 7),
                (a, 1),
                (a, 2),
                (b, 1),
                (me, 1)
            ]
        );
    }

    #[test]
    fn columns_agree_or_are_refused() {
        for p in [
            PoolSource::Instance,
            PoolSource::Guild { guild_id: 3 },
            PoolSource::Personal { user_id: 4 },
        ] {
            assert_eq!(
                PoolSource::from_columns(p.as_str(), p.guild_id(), p.owner_id()),
                Some(p)
            );
        }
        assert_eq!(PoolSource::from_columns("guild", None, None), None);
        assert_eq!(PoolSource::from_columns("instance", Some(1), None), None);
        assert_eq!(PoolSource::from_columns("personal", Some(1), Some(2)), None);
    }

    #[test]
    fn the_wire_shape_is_tagged_with_string_ids() {
        let g: PoolSource = serde_json::from_str(r#"{"kind":"guild","guild_id":"77"}"#).unwrap();
        assert_eq!(g, PoolSource::Guild { guild_id: 77 });
        assert_eq!(
            serde_json::to_string(&g).unwrap(),
            r#"{"kind":"guild","guild_id":"77"}"#
        );
        let p: PoolSource = serde_json::from_str(r#"{"kind":"personal","user_id":5}"#).unwrap();
        assert_eq!(p, PoolSource::Personal { user_id: 5 });
        let i: PoolSource = serde_json::from_str(r#"{"kind":"instance"}"#).unwrap();
        assert_eq!(i, PoolSource::Instance);
    }
}

//! SFU node selection (VOX-SIG-004). A channel's room is pinned to one node, chosen by
//! rendezvous hashing over the configured endpoints, so adding or removing a node only
//! moves the rooms that hashed to it.

use sr_core::ChannelId;

fn mix(mut x: u64) -> u64 {
    // splitmix64 finaliser.
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The endpoint that owns `channel`; `None` when the list is empty.
pub fn select_endpoint(channel: ChannelId, endpoints: &[String]) -> Option<&str> {
    endpoints
        .iter()
        .max_by_key(|e| mix(fnv(e) ^ mix(channel.get())))
        .map(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_and_spread() {
        let eps: Vec<String> = (0..3)
            .map(|i| format!("wss://sfu{i}.example/voice"))
            .collect();
        let pick = |c: u64| select_endpoint(ChannelId::new(c), &eps).unwrap().to_owned();
        assert_eq!(pick(5), pick(5));
        let used: std::collections::HashSet<_> = (0..200).map(pick).collect();
        assert_eq!(used.len(), 3);
        assert!(select_endpoint(ChannelId::new(1), &[]).is_none());
    }

    #[test]
    fn removing_a_node_moves_only_its_rooms() {
        let all: Vec<String> = (0..3).map(|i| format!("sfu{i}")).collect();
        let fewer = all[..2].to_vec();
        for c in 0..200u64 {
            let before = select_endpoint(ChannelId::new(c), &all).unwrap();
            if before != "sfu2" {
                assert_eq!(select_endpoint(ChannelId::new(c), &fewer).unwrap(), before);
            }
        }
    }
}

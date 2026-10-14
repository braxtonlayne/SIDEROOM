//! SFU settings, independent of the application config crate.

use std::net::IpAddr;
use std::time::Duration;

/// Most UDP sockets the SFU binds out of the configured port range. Each socket costs a
/// file descriptor and a reader task; participants are spread across them.
pub const MAX_UDP_SOCKETS: usize = 256;

#[derive(Debug, Clone)]
pub struct SfuConfig {
    /// Interface the media sockets bind to.
    pub bind_ip: IpAddr,
    /// First and last UDP port of the media range (inclusive).
    pub udp_port_min: u16,
    pub udp_port_max: u16,
    /// ICE-TCP (passive, RFC 6544 with RFC 4571 framing) listener port, if enabled.
    pub tcp_port: Option<u16>,
    /// Addresses advertised as host candidates (the node's public/LAN IPs). The first is
    /// also the address media datagrams are treated as having arrived on.
    pub announce_ips: Vec<IpAddr>,
    pub max_participants: usize,
    pub max_room_size: usize,
    /// How long a participant survives without a voice WebSocket (RESUME window).
    pub resume_window: Duration,
    /// Heartbeat interval told to clients; the server closes after 2.5 missed intervals.
    pub heartbeat_interval: Duration,
    /// Shared secret for join tokens (`sr_voice_proto::TokenSigner`).
    pub token_secret: Vec<u8>,
}

impl Default for SfuConfig {
    fn default() -> Self {
        Self {
            bind_ip: IpAddr::from([0, 0, 0, 0]),
            udp_port_min: 50_000,
            udp_port_max: 50_099,
            tcp_port: None,
            announce_ips: vec![IpAddr::from([127, 0, 0, 1])],
            max_participants: 2_000,
            max_room_size: 99,
            resume_window: Duration::from_secs(20),
            heartbeat_interval: Duration::from_millis(13_750),
            token_secret: Vec::new(),
        }
    }
}

//! The SSRF policy: which URLs and addresses outbound fetches may touch.
//!
//! Everything here is pure so it can be tested as a table. The rules are applied
//! to the URL before any DNS lookup, to every address a lookup returns, and again
//! on every redirect hop.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use url::{Host, Url};

/// Why a URL or address was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Blocked {
    #[error("only http and https URLs can be fetched")]
    Scheme,
    #[error("the URL has no host")]
    NoHost,
    #[error("URLs with embedded credentials are not fetched")]
    Credentials,
    #[error("the URL is too long")]
    TooLong,
    #[error("the host is on the operator deny list")]
    DeniedHost,
    #[error("the host is not on the operator allow list")]
    NotAllowedHost,
    #[error("the host resolves to a non-public address ({0})")]
    PrivateAddress(IpAddr),
    #[error("the host did not resolve to any address")]
    NoAddress,
}

/// Longest URL the fetcher accepts.
pub const MAX_URL_LEN: usize = 2048;

/// The deny and allow host lists as one value, so a live handle can swap both together.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostLists {
    pub deny_hosts: Vec<String>,
    pub allow_hosts: Vec<String>,
}

/// Operator-controlled knobs of the policy.
#[derive(Debug, Clone, Default)]
pub struct Policy {
    /// Hosts (and all their subdomains) that are never fetched. Compared in
    /// lower case without a trailing dot.
    pub deny_hosts: Vec<String>,
    /// When not empty, only these hosts (and their subdomains) are fetched, and
    /// literal IP addresses are refused. The deny list still wins.
    pub allow_hosts: Vec<String>,
    /// TEST ONLY. Disables the address check so a loopback test server can be
    /// fetched. Must stay `false` in every real deployment.
    pub allow_private_for_tests: bool,
    /// Operator opt-in for integrations that legitimately live on the private network
    /// (for example a self-hosted esplora node). Off by default; only ever set from an
    /// explicit operator setting, never from user input.
    pub allow_private_hosts: bool,
    /// When set, the deny and allow lists are read from this handle on every check instead of
    /// from `deny_hosts` / `allow_hosts`, so a saved change applies without a restart (D-300).
    pub live_hosts: Option<crate::live::Live<HostLists>>,
}

impl Policy {
    /// Run `f` with the deny and allow lists in force right now.
    fn with_lists<R>(&self, f: impl FnOnce(&[String], &[String]) -> R) -> R {
        match &self.live_hosts {
            Some(l) => {
                let lists = l.load();
                f(&lists.deny_hosts, &lists.allow_hosts)
            }
            None => f(&self.deny_hosts, &self.allow_hosts),
        }
    }

    /// Check the syntactic parts of `url` (scheme, host, credentials, deny list).
    pub fn check_url(&self, url: &Url) -> Result<(), Blocked> {
        if url.as_str().len() > MAX_URL_LEN {
            return Err(Blocked::TooLong);
        }
        if !matches!(url.scheme(), "http" | "https") {
            return Err(Blocked::Scheme);
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(Blocked::Credentials);
        }
        match url.host() {
            None => Err(Blocked::NoHost),
            Some(Host::Domain(d)) => self.with_lists(|deny, allow| {
                if matches_list(deny, d) {
                    Err(Blocked::DeniedHost)
                } else if !allow.is_empty() && !matches_list(allow, d) {
                    Err(Blocked::NotAllowedHost)
                } else {
                    Ok(())
                }
            }),
            // Literal addresses are checked for range here and need no lookup.
            Some(Host::Ipv4(ip)) => self
                .check_ip(IpAddr::V4(ip))
                .and_then(|()| self.no_allow_list()),
            Some(Host::Ipv6(ip)) => self
                .check_ip(IpAddr::V6(ip))
                .and_then(|()| self.no_allow_list()),
        }
    }

    /// Whether `host` or one of its parents is on the deny list.
    pub fn is_denied(&self, host: &str) -> bool {
        self.with_lists(|deny, _| matches_list(deny, host))
    }

    /// A literal IP address cannot be on a host allow list.
    fn no_allow_list(&self) -> Result<(), Blocked> {
        if self.with_lists(|_, allow| allow.is_empty()) {
            Ok(())
        } else {
            Err(Blocked::NotAllowedHost)
        }
    }

    /// Check one resolved address.
    pub fn check_ip(&self, ip: IpAddr) -> Result<(), Blocked> {
        if !(self.allow_private_for_tests || self.allow_private_hosts) && is_forbidden_ip(ip) {
            Err(Blocked::PrivateAddress(ip))
        } else {
            Ok(())
        }
    }

    /// Check every address of a lookup. One bad address refuses the whole host: a
    /// name that is public for some answers and private for others is hostile.
    pub fn check_all(&self, ips: &[IpAddr]) -> Result<(), Blocked> {
        if ips.is_empty() {
            return Err(Blocked::NoAddress);
        }
        ips.iter().try_for_each(|ip| self.check_ip(*ip))
    }
}

/// Whether `host` equals, or is a subdomain of, an entry of `list` (entries may be
/// written `*.example.org`; case and a trailing dot do not matter).
fn matches_list(list: &[String], host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    list.iter().any(|d| {
        let d = d
            .trim()
            .trim_start_matches("*.")
            .trim_end_matches('.')
            .to_ascii_lowercase();
        !d.is_empty()
            && (host == d
                || host
                    .strip_suffix(d.as_str())
                    .is_some_and(|rest| rest.ends_with('.')))
    })
}

/// True for every address that is not a normal public unicast address.
pub fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => forbidden_v4(v4),
        IpAddr::V6(v6) => forbidden_v6(v6),
    }
}

fn forbidden_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local() // 169.254/16, includes the 169.254.169.254 metadata address
        || ip.is_broadcast()
        || ip.is_multicast()
        || a == 0 // "this network"
        || (a == 100 && (64..=127).contains(&b)) // CGNAT 100.64/10 (also 100.100.100.200 metadata)
        || (a == 192 && b == 0 && c == 0) // IETF protocol assignments (192.0.0.192 metadata)
        || (a == 192 && b == 0 && c == 2) // TEST-NET-1
        || (a == 198 && (b == 18 || b == 19)) // benchmarking
        || (a == 198 && b == 51 && c == 100) // TEST-NET-2
        || (a == 203 && b == 0 && c == 113) // TEST-NET-3
        || a >= 240 // reserved
}

fn forbidden_v6(ip: Ipv6Addr) -> bool {
    if ip.is_unspecified() || ip.is_loopback() || ip.is_multicast() {
        return true;
    }
    let s = ip.segments();
    // IPv4-mapped (::ffff:a.b.c.d) and the deprecated IPv4-compatible (::a.b.c.d) forms.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return forbidden_v4(v4);
    }
    if s[..6] == [0, 0, 0, 0, 0, 0] {
        return forbidden_v4(embedded_v4(s[6], s[7]));
    }
    // NAT64 (64:ff9b::/96 and 64:ff9b:1::/48): the embedded address is what is reached.
    if s[0] == 0x64 && s[1] == 0xff9b {
        return s[2] != 0 || forbidden_v4(embedded_v4(s[6], s[7]));
    }
    // 6to4 (2002::/16) embeds the IPv4 address right after the prefix.
    if s[0] == 0x2002 {
        return forbidden_v4(embedded_v4(s[1], s[2]));
    }
    (s[0] & 0xfe00) == 0xfc00 // unique local fc00::/7 (also fd00:ec2::254, the AWS v6 metadata address)
        || (s[0] & 0xffc0) == 0xfe80 // link local fe80::/10
        || (s[0] & 0xffc0) == 0xfec0 // deprecated site local fec0::/10
        || (s[0] == 0x2001 && s[1] == 0) // Teredo 2001::/32
        || (s[0] == 0x2001 && s[1] == 0x0db8) // documentation
        || (s[0] == 0x0100 && s[1] == 0 && s[2] == 0 && s[3] == 0) // discard-only 100::/64
}

fn embedded_v4(hi: u16, lo: u16) -> Ipv4Addr {
    let [a, b] = hi.to_be_bytes();
    let [c, d] = lo.to_be_bytes();
    Ipv4Addr::new(a, b, c, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn forbidden_addresses_table() {
        for bad in [
            "127.0.0.1",
            "127.255.255.254",
            "0.0.0.0",
            "10.0.0.1",
            "10.255.255.255",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.1",
            "169.254.169.254",
            "169.254.0.1",
            "100.64.0.1",
            "100.100.100.200",
            "100.127.255.255",
            "192.0.0.192",
            "198.18.0.1",
            "224.0.0.1",
            "255.255.255.255",
            "240.0.0.1",
            "203.0.113.9",
            "::",
            "::1",
            "fc00::1",
            "fd00:ec2::254",
            "fe80::1",
            "fec0::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:10.1.2.3",
            "::ffff:169.254.169.254",
            "::127.0.0.1",
            "64:ff9b::7f00:1",
            "64:ff9b::a9fe:a9fe",
            "2002:7f00:1::1",
            "2002:a9fe:a9fe::1",
            "2001:0:4136:e378:8000:63bf:3fff:fdd2",
            "2001:db8::1",
        ] {
            assert!(is_forbidden_ip(ip(bad)), "{bad} must be forbidden");
        }
    }

    #[test]
    fn public_addresses_pass() {
        for good in [
            "1.1.1.1",
            "8.8.8.8",
            "93.184.216.34",
            "172.32.0.1",
            "100.63.255.255",
            "100.128.0.1",
            "2606:4700:4700::1111",
            "2a00:1450:4001:81b::200e",
            "::ffff:8.8.8.8",
            "64:ff9b::808:808",
            "2002:808:808::1",
        ] {
            assert!(!is_forbidden_ip(ip(good)), "{good} must be allowed");
        }
    }

    #[test]
    fn live_lists_replace_the_static_ones_on_every_check() {
        let live = crate::live::Live::new(HostLists::default());
        let p = Policy {
            live_hosts: Some(live.clone()),
            deny_hosts: vec!["ignored.example".into()],
            ..Policy::default()
        };
        let u = |s: &str| Url::parse(s).unwrap();
        assert_eq!(p.check_url(&u("https://ignored.example/")), Ok(()));
        live.store(HostLists {
            deny_hosts: vec!["evil.example".into()],
            allow_hosts: vec![],
        });
        assert_eq!(
            p.check_url(&u("https://www.evil.example/")),
            Err(Blocked::DeniedHost)
        );
        live.store(HostLists {
            deny_hosts: vec![],
            allow_hosts: vec!["only.example".into()],
        });
        assert_eq!(
            p.check_url(&u("https://other.example/")),
            Err(Blocked::NotAllowedHost)
        );
        assert_eq!(p.check_url(&u("https://only.example/")), Ok(()));
    }

    #[test]
    fn url_table() {
        let p = Policy {
            deny_hosts: vec!["evil.example".into(), "*.tracker.test".into()],
            ..Policy::default()
        };
        let private = |s: &str| Err(Blocked::PrivateAddress(ip(s)));
        let cases: Vec<(&str, Result<(), Blocked>)> = vec![
            ("https://example.com/a", Ok(())),
            ("http://example.com:8080/", Ok(())),
            ("ftp://example.com/", Err(Blocked::Scheme)),
            ("file:///etc/passwd", Err(Blocked::Scheme)),
            ("gopher://example.com/", Err(Blocked::Scheme)),
            ("https://user:pw@example.com/", Err(Blocked::Credentials)),
            ("http://127.0.0.1/", private("127.0.0.1")),
            ("http://[::1]/", private("::1")),
            ("http://10.0.0.5/x", private("10.0.0.5")),
            (
                "http://169.254.169.254/latest/meta-data",
                private("169.254.169.254"),
            ),
            // WHATWG normalisation turns the tricky IPv4 spellings into real addresses.
            ("http://2130706433/", private("127.0.0.1")),
            ("http://0x7f.1/", private("127.0.0.1")),
            ("http://127.1/", private("127.0.0.1")),
            ("http://[::ffff:7f00:1]/", private("::ffff:127.0.0.1")),
            ("http://evil.example/", Err(Blocked::DeniedHost)),
            ("http://cdn.evil.example./", Err(Blocked::DeniedHost)),
            ("http://a.b.tracker.test/", Err(Blocked::DeniedHost)),
            ("http://notevil.example/", Ok(())),
        ];
        for (u, want) in cases {
            let got = p.check_url(&Url::parse(u).unwrap());
            assert_eq!(got, want, "{u}");
        }
    }

    #[test]
    fn allow_list_limits_hosts_and_the_deny_list_still_wins() {
        let p = Policy {
            allow_hosts: vec!["images.example".into(), "*.cdn.test".into()],
            deny_hosts: vec!["bad.cdn.test".into()],
            ..Policy::default()
        };
        let check = |u: &str| p.check_url(&Url::parse(u).unwrap());
        assert_eq!(check("https://images.example/a.png"), Ok(()));
        assert_eq!(check("https://x.images.example/a.png"), Ok(()));
        assert_eq!(check("https://a.cdn.test/a.png"), Ok(()));
        assert_eq!(
            check("https://other.example/a.png"),
            Err(Blocked::NotAllowedHost)
        );
        assert_eq!(
            check("https://notimages.example/a.png"),
            Err(Blocked::NotAllowedHost)
        );
        assert_eq!(
            check("https://bad.cdn.test/a.png"),
            Err(Blocked::DeniedHost)
        );
        // Literal addresses are never on a host list.
        assert_eq!(check("http://8.8.8.8/a.png"), Err(Blocked::NotAllowedHost));
        // An empty allow list allows every public host (the default).
        assert!(
            Policy::default()
                .check_url(&Url::parse("https://other.example/").unwrap())
                .is_ok()
        );
    }

    #[test]
    fn one_bad_answer_refuses_the_host() {
        let p = Policy::default();
        assert_eq!(p.check_all(&[]), Err(Blocked::NoAddress));
        assert!(p.check_all(&[ip("8.8.8.8")]).is_ok());
        assert_eq!(
            p.check_all(&[ip("8.8.8.8"), ip("10.0.0.1")]),
            Err(Blocked::PrivateAddress(ip("10.0.0.1")))
        );
    }

    #[test]
    fn the_test_flag_disables_only_the_address_check() {
        let p = Policy {
            allow_private_for_tests: true,
            ..Policy::default()
        };
        assert!(
            p.check_url(&Url::parse("http://127.0.0.1:9/").unwrap())
                .is_ok()
        );
        assert_eq!(
            p.check_url(&Url::parse("ftp://127.0.0.1/").unwrap()),
            Err(Blocked::Scheme)
        );
    }
}

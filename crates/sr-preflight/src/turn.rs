//! A TURN allocation against coturn over UDP (RFC 5766 with the long-term credential mechanism of
//! RFC 5389): Allocate without credentials, take REALM and NONCE from the 401, Allocate again with
//! USERNAME and MESSAGE-INTEGRITY, then release it with a zero-lifetime Refresh.

use crate::md5::md5;
use hmac::{Hmac, KeyInit, Mac};
use sha1::{Digest, Sha1};
use std::{
    net::SocketAddr,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use tokio::net::UdpSocket;

const COOKIE: u32 = 0x2112_A442;
const ALLOCATE: u16 = 0x0003;
const ALLOCATE_OK: u16 = 0x0103;
const ALLOCATE_ERR: u16 = 0x0113;
const REFRESH: u16 = 0x0004;
const A_USERNAME: u16 = 0x0006;
const A_INTEGRITY: u16 = 0x0008;
const A_ERROR: u16 = 0x0009;
const A_REALM: u16 = 0x0014;
const A_NONCE: u16 = 0x0015;
const A_LIFETIME: u16 = 0x000D;
const A_TRANSPORT: u16 = 0x0019;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn txid() -> [u8; 12] {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut h = Sha1::new();
    h.update(nanos.to_le_bytes());
    h.update(COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    h.update(std::process::id().to_le_bytes());
    let d = h.finalize();
    let mut out = [0u8; 12];
    out.copy_from_slice(&d[..12]);
    out
}

fn push_attr(buf: &mut Vec<u8>, ty: u16, val: &[u8]) {
    buf.extend_from_slice(&ty.to_be_bytes());
    buf.extend_from_slice(&(val.len() as u16).to_be_bytes());
    buf.extend_from_slice(val);
    while buf.len() % 4 != 0 {
        buf.push(0);
    }
}

/// Credentials and the values the server handed out in its 401.
#[derive(Clone)]
pub(crate) struct Auth {
    pub username: String,
    pub realm: String,
    pub nonce: Vec<u8>,
    pub key: [u8; 16],
}

impl Auth {
    pub(crate) fn new(username: &str, password: &str, realm: &[u8], nonce: &[u8]) -> Self {
        let realm = String::from_utf8_lossy(realm).into_owned();
        let key = md5(format!("{username}:{realm}:{password}").as_bytes());
        Self {
            username: username.to_owned(),
            realm,
            nonce: nonce.to_vec(),
            key,
        }
    }
}

/// Build a request. With `auth` the message carries USERNAME, REALM, NONCE and a MESSAGE-INTEGRITY.
pub(crate) fn request(
    ty: u16,
    id: &[u8; 12],
    attrs: &[(u16, Vec<u8>)],
    auth: Option<&Auth>,
) -> Vec<u8> {
    let mut body = Vec::new();
    for (t, v) in attrs {
        push_attr(&mut body, *t, v);
    }
    if let Some(a) = auth {
        push_attr(&mut body, A_USERNAME, a.username.as_bytes());
        push_attr(&mut body, A_REALM, a.realm.as_bytes());
        push_attr(&mut body, A_NONCE, &a.nonce);
    }
    let extra = if auth.is_some() { 24 } else { 0 };
    let mut msg = Vec::with_capacity(20 + body.len() + extra);
    msg.extend_from_slice(&ty.to_be_bytes());
    msg.extend_from_slice(&((body.len() + extra) as u16).to_be_bytes());
    msg.extend_from_slice(&COOKIE.to_be_bytes());
    msg.extend_from_slice(id);
    msg.extend_from_slice(&body);
    if let Some(a) = auth
        && let Ok(mut mac) = Hmac::<Sha1>::new_from_slice(&a.key)
    {
        mac.update(&msg);
        let tag = mac.finalize().into_bytes();
        push_attr(&mut msg, A_INTEGRITY, &tag);
    }
    msg
}

/// A parsed message.
#[derive(Debug)]
pub(crate) struct Reply {
    pub ty: u16,
    pub attrs: Vec<(u16, Vec<u8>)>,
}

impl Reply {
    pub(crate) fn attr(&self, ty: u16) -> Option<&[u8]> {
        self.attrs
            .iter()
            .find(|(t, _)| *t == ty)
            .map(|(_, v)| v.as_slice())
    }

    /// The ERROR-CODE as `class * 100 + number`.
    pub(crate) fn error_code(&self) -> Option<u16> {
        let v = self.attr(A_ERROR)?;
        (v.len() >= 4).then(|| u16::from(v[2] & 7) * 100 + u16::from(v[3]))
    }
}

pub(crate) fn parse(buf: &[u8], id: &[u8; 12]) -> Option<Reply> {
    if buf.len() < 20 || buf[4..8] != COOKIE.to_be_bytes() || buf[8..20] != id[..] {
        return None;
    }
    let ty = u16::from_be_bytes([buf[0], buf[1]]);
    let len = usize::from(u16::from_be_bytes([buf[2], buf[3]]));
    let body = buf.get(20..20 + len)?;
    let mut attrs = Vec::new();
    let mut at = 0;
    while at + 4 <= body.len() {
        let t = u16::from_be_bytes([body[at], body[at + 1]]);
        let l = usize::from(u16::from_be_bytes([body[at + 2], body[at + 3]]));
        let v = body.get(at + 4..at + 4 + l)?;
        attrs.push((t, v.to_vec()));
        at += 4 + l.div_ceil(4) * 4;
    }
    Some(Reply { ty, attrs })
}

async fn exchange(
    sock: &UdpSocket,
    msg: &[u8],
    id: &[u8; 12],
    wait: Duration,
) -> Result<Reply, String> {
    let mut buf = vec![0u8; 2048];
    for _ in 0..2 {
        sock.send(msg).await.map_err(|e| format!("send: {e}"))?;
        match tokio::time::timeout(wait, sock.recv(&mut buf)).await {
            Ok(Ok(n)) => {
                if let Some(r) = parse(&buf[..n], id) {
                    return Ok(r);
                }
            }
            Ok(Err(e)) => return Err(format!("receive: {e}")),
            Err(_) => {}
        }
    }
    Err("no answer from the TURN server (UDP, 2 tries)".into())
}

/// Run the allocation handshake against `addr` and release the allocation again.
pub async fn allocate(
    addr: SocketAddr,
    username: &str,
    password: &str,
    wait: Duration,
) -> Result<(), String> {
    let bind = if addr.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let sock = UdpSocket::bind(bind)
        .await
        .map_err(|e| format!("bind: {e}"))?;
    sock.connect(addr)
        .await
        .map_err(|e| format!("connect: {e}"))?;
    let transport = vec![(A_TRANSPORT, vec![17, 0, 0, 0])];

    let id1 = txid();
    let first = exchange(
        &sock,
        &request(ALLOCATE, &id1, &transport, None),
        &id1,
        wait,
    )
    .await?;
    match first.ty {
        ALLOCATE_OK => return Ok(()),
        ALLOCATE_ERR if first.error_code() == Some(401) => {}
        ALLOCATE_ERR => {
            return Err(format!(
                "refused with error {}",
                first.error_code().unwrap_or(0)
            ));
        }
        other => return Err(format!("unexpected answer type {other:#06x}")),
    }
    let (Some(realm), Some(nonce)) = (first.attr(A_REALM), first.attr(A_NONCE)) else {
        return Err("the 401 carried no REALM and NONCE".into());
    };
    let auth = Auth::new(username, password, realm, nonce);

    let id2 = txid();
    let second = exchange(
        &sock,
        &request(ALLOCATE, &id2, &transport, Some(&auth)),
        &id2,
        wait,
    )
    .await?;
    match second.ty {
        ALLOCATE_OK => {}
        ALLOCATE_ERR => {
            let code = second.error_code().unwrap_or(0);
            return Err(if code == 401 {
                "credentials rejected (401): is voice.turn.secret the coturn static-auth-secret?"
                    .to_owned()
            } else {
                format!("allocation refused with error {code}")
            });
        }
        other => return Err(format!("unexpected answer type {other:#06x}")),
    }
    // Release the allocation; failure here does not matter, it expires on its own.
    let id3 = txid();
    let release = request(
        REFRESH,
        &id3,
        &[(A_LIFETIME, vec![0, 0, 0, 0])],
        Some(&auth),
    );
    let _ = exchange(&sock, &release, &id3, wait).await;
    Ok(())
}

/// Split `turn:host:3478?transport=udp` into `(scheme, host, port, transport)`.
pub fn parse_url(url: &str) -> Option<(String, String, u16, Option<String>)> {
    let (scheme, rest) = url.split_once(':')?;
    if !matches!(scheme, "turn" | "turns" | "stun" | "stuns") {
        return None;
    }
    let (hostport, query) = match rest.split_once('?') {
        Some((h, q)) => (h, Some(q)),
        None => (rest, None),
    };
    let transport = query.and_then(|q| {
        q.split('&')
            .find_map(|kv| kv.strip_prefix("transport="))
            .map(str::to_ascii_lowercase)
    });
    let default_port = if scheme.ends_with('s') { 5349 } else { 3478 };
    let (host, port) = if let Some(r) = hostport.strip_prefix('[') {
        let (h, tail) = r.split_once(']')?;
        let p = tail.strip_prefix(':').map(str::parse).transpose().ok()?;
        (h.to_owned(), p.unwrap_or(default_port))
    } else if let Some((h, p)) = hostport.rsplit_once(':') {
        (h.to_owned(), p.parse().ok()?)
    } else {
        (hostport.to_owned(), default_port)
    };
    (!host.is_empty()).then_some((scheme.to_owned(), host, port, transport))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_split_into_parts() {
        assert_eq!(
            parse_url("turn:turn.example.org:3478?transport=udp"),
            Some((
                "turn".into(),
                "turn.example.org".into(),
                3478,
                Some("udp".into())
            ))
        );
        assert_eq!(
            parse_url("turns:t.example.org"),
            Some(("turns".into(), "t.example.org".into(), 5349, None))
        );
        assert_eq!(
            parse_url("turn:[::1]:3479"),
            Some(("turn".into(), "::1".into(), 3479, None))
        );
        assert_eq!(parse_url("https://x"), None);
        assert_eq!(parse_url("turn:"), None);
    }

    #[test]
    fn request_and_parse_round_trip() {
        let id = [7u8; 12];
        let a = Auth::new("1:2", "pw", b"realm", b"nonce");
        let m = request(ALLOCATE, &id, &[(A_TRANSPORT, vec![17, 0, 0, 0])], Some(&a));
        // The header length covers the attributes including the 24-byte integrity attribute.
        assert_eq!(usize::from(u16::from_be_bytes([m[2], m[3]])), m.len() - 20);
        let r = parse(&m, &id).unwrap();
        assert_eq!(r.ty, ALLOCATE);
        assert_eq!(r.attr(A_USERNAME), Some(&b"1:2"[..]));
        assert_eq!(r.attr(A_INTEGRITY).map(<[u8]>::len), Some(20));
        assert!(parse(&m, &[8u8; 12]).is_none());
    }

    /// A UDP socket that plays coturn: 401 with realm and nonce, then success when the integrity
    /// attribute matches the key derived from `password`.
    async fn fake_coturn(password: &'static str) -> SocketAddr {
        let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = sock.local_addr().unwrap();
        tokio::spawn(async move {
            let mut buf = [0u8; 2048];
            loop {
                let Ok((n, from)) = sock.recv_from(&mut buf).await else {
                    return;
                };
                let msg = &buf[..n];
                let mut id = [0u8; 12];
                id.copy_from_slice(&msg[8..20]);
                let ty = u16::from_be_bytes([msg[0], msg[1]]);
                let req = parse(msg, &id).unwrap();
                let reply = |ty: u16, attrs: Vec<(u16, Vec<u8>)>| {
                    let mut body = Vec::new();
                    for (t, v) in &attrs {
                        push_attr(&mut body, *t, v);
                    }
                    let mut m = Vec::new();
                    m.extend_from_slice(&ty.to_be_bytes());
                    m.extend_from_slice(&(body.len() as u16).to_be_bytes());
                    m.extend_from_slice(&COOKIE.to_be_bytes());
                    m.extend_from_slice(&id);
                    m.extend_from_slice(&body);
                    m
                };
                let out = match req.attr(A_INTEGRITY) {
                    None => reply(
                        ALLOCATE_ERR,
                        vec![
                            (A_ERROR, vec![0, 0, 4, 1]),
                            (A_REALM, b"sr.test".to_vec()),
                            (A_NONCE, b"abc123".to_vec()),
                        ],
                    ),
                    Some(tag) => {
                        let username = String::from_utf8_lossy(req.attr(A_USERNAME).unwrap());
                        let key = md5(format!("{username}:sr.test:{password}").as_bytes());
                        // Recompute the MAC over everything before the integrity attribute.
                        let cut = n - 24;
                        let mut head = msg[..cut].to_vec();
                        let len = (cut - 20 + 24) as u16;
                        head[2..4].copy_from_slice(&len.to_be_bytes());
                        let mut mac = Hmac::<Sha1>::new_from_slice(&key).unwrap();
                        mac.update(&head);
                        if mac.finalize().into_bytes().as_slice() == tag {
                            if ty == ALLOCATE {
                                reply(ALLOCATE_OK, vec![])
                            } else {
                                reply(0x0104, vec![])
                            }
                        } else {
                            reply(ALLOCATE_ERR, vec![(A_ERROR, vec![0, 0, 4, 1])])
                        }
                    }
                };
                let _ = sock.send_to(&out, from).await;
            }
        });
        addr
    }

    #[tokio::test]
    async fn allocation_succeeds_with_the_right_secret_and_fails_with_a_wrong_one() {
        let addr = fake_coturn("right").await;
        let wait = Duration::from_millis(500);
        allocate(addr, "1700000000:1", "right", wait)
            .await
            .expect("allocation");
        let err = allocate(addr, "1700000000:1", "wrong", wait)
            .await
            .unwrap_err();
        assert!(err.contains("credentials rejected"), "{err}");
    }

    #[tokio::test]
    async fn a_silent_port_times_out() {
        let sock = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let addr = sock.local_addr().unwrap();
        let err = allocate(addr, "u", "p", Duration::from_millis(100))
            .await
            .unwrap_err();
        assert!(err.contains("no answer"), "{err}");
    }
}

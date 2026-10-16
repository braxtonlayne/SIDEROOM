//! `sr-server healthcheck`: a container health probe that needs no shell and no curl.
//!
//! The production image is distroless, so Docker cannot run `curl /readyz` inside it. This module is
//! what `HEALTHCHECK` / the compose `healthcheck` runs instead: one plain HTTP/1.0 request to the
//! server's own `/readyz` over loopback, exit code 0 for a 200 and 1 for anything else (XC-OPS-001).

use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs},
    time::Duration,
};

/// How long the whole probe may take before it counts as a failure.
pub const TIMEOUT: Duration = Duration::from_secs(3);

/// The loopback address that reaches a listener bound to `bind` (`0.0.0.0:8080` and `[::]:8080` both
/// answer on `127.0.0.1`).
pub fn loopback_of(bind: &str) -> anyhow::Result<SocketAddr> {
    let addr = bind
        .to_socket_addrs()
        .map_err(|e| anyhow::anyhow!("server.bind {bind:?} is not an address: {e}"))?
        .next()
        .ok_or_else(|| anyhow::anyhow!("server.bind {bind:?} resolves to nothing"))?;
    Ok(if addr.ip().is_unspecified() {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), addr.port())
    } else {
        addr
    })
}

/// `GET path` over HTTP/1.0 and the status code of the answer.
pub fn probe(addr: SocketAddr, path: &str, timeout: Duration) -> std::io::Result<u16> {
    let mut s = TcpStream::connect_timeout(&addr, timeout)?;
    s.set_read_timeout(Some(timeout))?;
    s.set_write_timeout(Some(timeout))?;
    // One write, so the request is a single segment even to a server that reads once.
    let request = format!(
        "GET {path} HTTP/1.0\r\nHost: localhost\r\nUser-Agent: sr-server-healthcheck\r\nConnection: close\r\n\r\n"
    );
    s.write_all(request.as_bytes())?;
    // The status line is all that matters; the head of the response always carries it.
    let mut head = [0u8; 64];
    let mut got = 0;
    while got < head.len() {
        match s.read(&mut head[got..])? {
            0 => break,
            n => got += n,
        }
        if head[..got].contains(&b'\n') {
            break;
        }
    }
    let line = String::from_utf8_lossy(&head[..got]);
    line.split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("not an HTTP status line: {:?}", line.trim()),
            )
        })
}

/// Run the check against the configured listener: `Ok` only for 200 on `/readyz`.
pub fn run(bind: &str) -> anyhow::Result<()> {
    let addr = loopback_of(bind)?;
    let status = probe(addr, "/readyz", TIMEOUT)
        .map_err(|e| anyhow::anyhow!("GET /readyz on {addr}: {e}"))?;
    anyhow::ensure!(status == 200, "GET /readyz on {addr} answered {status}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A one-shot server that answers the first request with `response`.
    fn serve_once(response: &'static str) -> SocketAddr {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut c, _)) = l.accept() {
                // Read the whole request head before answering, as a real server does; closing with unread
                // input would reset the connection and swallow the response.
                let mut head = Vec::new();
                let mut buf = [0u8; 256];
                while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                    match c.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => head.extend_from_slice(&buf[..n]),
                    }
                }
                let _ = c.write_all(response.as_bytes());
            }
        });
        addr
    }

    #[test]
    fn reads_the_status_of_the_answer() {
        let ok = serve_once("HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\n{}");
        assert_eq!(probe(ok, "/readyz", TIMEOUT).unwrap(), 200);
        let down = serve_once("HTTP/1.1 503 Service Unavailable\r\n\r\n");
        assert_eq!(probe(down, "/readyz", TIMEOUT).unwrap(), 503);
    }

    #[test]
    fn run_demands_a_200() {
        let ok = serve_once("HTTP/1.1 200 OK\r\n\r\n");
        assert!(run(&ok.to_string()).is_ok());
        let down = serve_once("HTTP/1.1 503 Service Unavailable\r\n\r\n");
        let err = run(&down.to_string()).unwrap_err().to_string();
        assert!(err.contains("503"), "{err}");
    }

    #[test]
    fn a_closed_port_and_garbage_both_fail() {
        let free = {
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap()
        };
        assert!(probe(free, "/readyz", Duration::from_millis(500)).is_err());
        let junk = serve_once("not http at all\r\n");
        assert!(probe(junk, "/readyz", TIMEOUT).is_err());
    }

    #[test]
    fn an_unspecified_bind_is_probed_on_loopback() {
        assert_eq!(
            loopback_of("0.0.0.0:8080").unwrap(),
            "127.0.0.1:8080".parse().unwrap()
        );
        assert_eq!(
            loopback_of("[::]:9000").unwrap(),
            "127.0.0.1:9000".parse().unwrap()
        );
        assert_eq!(
            loopback_of("10.1.2.3:80").unwrap(),
            "10.1.2.3:80".parse().unwrap()
        );
        assert!(loopback_of("nonsense").is_err());
    }
}

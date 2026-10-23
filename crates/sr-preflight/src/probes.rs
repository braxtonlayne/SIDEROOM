//! The real probes: Postgres, the network and the local file system.

use crate::checks::{DbProbe, HostProbe, HttpReply, MigrationState, ModeResult, NetProbe};
use async_trait::async_trait;
use sqlx::PgPool;
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::{TcpStream, lookup_host},
};

const NET_TIMEOUT: Duration = Duration::from_secs(8);

/// The database checks over a pool.
pub struct PgDb(pub PgPool);

#[async_trait]
impl DbProbe for PgDb {
    async fn ping(&self) -> Result<(), String> {
        sqlx::query("SELECT 1")
            .execute(&self.0)
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    async fn migrations(&self) -> Result<MigrationState, String> {
        let s = sr_db::migration_status(&self.0)
            .await
            .map_err(|e| e.to_string())?;
        Ok(MigrationState {
            applied: s.applied.len(),
            pending: s.pending.len(),
            failed: s.failed,
            unknown: s.unknown,
        })
    }

    async fn now_ms(&self) -> Result<i64, String> {
        let ms: f64 =
            sqlx::query_scalar("SELECT (EXTRACT(EPOCH FROM clock_timestamp()) * 1000)::float8")
                .fetch_one(&self.0)
                .await
                .map_err(|e| e.to_string())?;
        Ok(ms as i64)
    }

    async fn vapid_present(&self) -> Result<bool, String> {
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM push_vapid_key)")
            .fetch_one(&self.0)
            .await
            .map_err(|e| e.to_string())
    }
}

/// TCP, HTTP and TURN from this machine.
pub struct RealNet;

#[async_trait]
impl NetProbe for RealNet {
    async fn tcp(&self, host: &str, port: u16, banner: bool) -> Result<Option<String>, String> {
        let work = async {
            let stream = TcpStream::connect((host, port))
                .await
                .map_err(|e| e.to_string())?;
            if !banner {
                return Ok(None);
            }
            let mut line = String::new();
            let mut r = BufReader::new(stream);
            r.read_line(&mut line).await.map_err(|e| e.to_string())?;
            Ok(Some(line.trim().to_owned()))
        };
        tokio::time::timeout(NET_TIMEOUT, work)
            .await
            .map_err(|_| "timed out".to_owned())?
    }

    async fn http_get(&self, url: &str, insecure: bool) -> Result<HttpReply, String> {
        let client = reqwest::Client::builder()
            .timeout(NET_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .danger_accept_invalid_certs(insecure)
            .user_agent("sideroom-preflight")
            .build()
            .map_err(|e| e.to_string())?;
        let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
        let status = resp.status().as_u16();
        let headers = resp
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_owned(),
                    String::from_utf8_lossy(v.as_bytes()).into_owned(),
                )
            })
            .collect();
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
        let cut = bytes.len().min(256);
        Ok(HttpReply {
            status,
            headers,
            body: String::from_utf8_lossy(&bytes[..cut]).into_owned(),
        })
    }

    async fn turn_allocate(
        &self,
        host: &str,
        port: u16,
        username: &str,
        password: &str,
    ) -> Result<(), String> {
        let addr: SocketAddr = lookup_host((host, port))
            .await
            .map_err(|e| format!("cannot resolve {host}: {e}"))?
            .next()
            .ok_or_else(|| format!("{host} has no address"))?;
        crate::turn::allocate(addr, username, password, Duration::from_secs(3)).await
    }
}

/// The local file system and clock.
pub struct RealHost;

impl HostProbe for RealHost {
    fn disk(&self, path: &str) -> Option<(u64, u64)> {
        let total = fs4::total_space(path).ok()?;
        let free = fs4::available_space(path).ok()?;
        Some((total, free))
    }

    #[cfg(unix)]
    fn mode(&self, path: &str) -> ModeResult {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(path) {
            Ok(m) => ModeResult::Mode(m.permissions().mode() & 0o777),
            Err(_) => ModeResult::Missing,
        }
    }

    #[cfg(not(unix))]
    fn mode(&self, path: &str) -> ModeResult {
        if std::path::Path::new(path).exists() {
            ModeResult::Unsupported
        } else {
            ModeResult::Missing
        }
    }

    fn now_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
    }
}

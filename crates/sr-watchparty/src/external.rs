//! Media that is not in the Cinema catalog (PRF-UNF-001): a provider's privacy-enhanced embed or a direct file.
//!
//! For YouTube, Vimeo and direct files the server fetches nothing and loads no provider script. It only decides
//! whether the address is one a member's browser may be sent to, and stores the canonical form. Each member's
//! browser loads it after a click, and the iframe's own `postMessage` commands (YouTube, Vimeo) or the native
//! player carry the sync.
//!
//! A link to an operator-listed PeerTube instance (D-515, D-516) is the one case the server fetches: once, at party
//! start or media change, it asks the instance's public API (`/api/v1/videos/<id>`) which file or HLS playlist the
//! video has, through `sr-unfurl`'s [`SafeFetcher`] (the link-preview SSRF policy). The party then plays that address
//! as `direct` media, and only when its host is the instance itself or listed in `SR_MEDIA_HOSTS`.

use std::sync::OnceLock;

use serde::Deserialize;
use sr_unfurl::SafeFetcher;
use sr_unfurl::embed::{self, PlayMode};
use sr_unfurl::fetch::{BodyClass, FetchError, MAX_JSON_BYTES};
use sr_unfurl::policy::Policy;
use url::Url;

use crate::error::{PartyError, PartyResult};
use crate::model::{ExternalInput, MediaFormat, MediaKind};

/// What the party stores for external media.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalMedia {
    pub kind: MediaKind,
    /// The embed address (`https://www.youtube-nocookie.com/embed/ID`), the media file or the HLS playlist.
    pub url: String,
    /// `youtube`, `vimeo`, `peertube` or `direct`.
    pub provider: String,
    pub title: String,
    /// `file` or `hls` for media the server resolved (PeerTube); `None` otherwise (a pasted file is a file).
    pub format: Option<MediaFormat>,
    /// The link the member pasted, when `url` was resolved from it.
    pub page_url: Option<String>,
}

const MAX_URL: usize = 2048;
const MAX_TITLE: usize = 120;
/// The largest web-video file a party picks (D-516).
const MAX_RESOLUTION: i64 = 1080;

pub const LIVE: &str = "That video is live; a party cannot seek it.";
pub const UNAVAILABLE: &str = "That video needs a password or is not available.";
pub const NO_SOURCE: &str = "That video has no file a party can play.";
pub const OFF_LIST_HOST: &str =
    "That instance keeps its video on a host the operator has not allowed.";
pub const NO_ANSWER: &str = "That instance did not answer. Try again in a moment.";
pub const BAD_ANSWER: &str = "That instance's answer could not be read.";
const CANNOT_SHARE: &str =
    "That player cannot be shared yet. Open it in a message to play it for yourself.";

fn peertube_hosts() -> &'static [String] {
    static HOSTS: OnceLock<Vec<String>> = OnceLock::new();
    HOSTS.get_or_init(embed::peertube_hosts_from_env)
}

fn bad(msg: &str) -> PartyError {
    PartyError::Invalid(msg.to_owned())
}

/// The PeerTube resolver (D-516): the fetcher (link-preview SSRF policy) and the operator's two host lists.
#[derive(Clone)]
pub struct PeerTube {
    fetcher: SafeFetcher,
    hosts: Vec<String>,
    media_hosts: Vec<String>,
}

impl PeerTube {
    /// `hosts` are `SR_PEERTUBE_HOSTS`, `media_hosts` are `SR_MEDIA_HOSTS` (both already parsed).
    pub fn new(fetcher: SafeFetcher, hosts: Vec<String>, media_hosts: Vec<String>) -> Self {
        Self {
            fetcher,
            hosts,
            media_hosts,
        }
    }

    /// The resolver with the instance's configured host lists.
    pub fn from_env(fetcher: SafeFetcher) -> Self {
        Self::new(
            fetcher,
            embed::peertube_hosts_from_env(),
            embed::media_hosts_from_env(),
        )
    }

    pub fn hosts(&self) -> &[String] {
        &self.hosts
    }
}

/// The checked link and the member's title, before any provider rule.
struct Checked {
    url: Url,
    given: Option<String>,
}

fn clean_title(raw: &str) -> Option<String> {
    let t: String = raw.chars().filter(|c| !c.is_control()).collect();
    let t: String = t.trim().chars().take(MAX_TITLE).collect();
    (!t.is_empty()).then_some(t)
}

/// Whether a host name is one every member's browser may be sent to: no single label, no local suffix.
fn public_name(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(d)) => {
            let d = d.trim_end_matches('.').to_ascii_lowercase();
            let local = [
                "localhost",
                "local",
                "internal",
                "lan",
                "home",
                "intranet",
                "arpa",
            ]
            .iter()
            .any(|suffix| d == *suffix || d.ends_with(&format!(".{suffix}")));
            d.contains('.') && !local
        }
        Some(_) => true,
        None => false,
    }
}

fn check(input: &ExternalInput) -> PartyResult<Checked> {
    let raw = input.url.trim();
    if raw.is_empty() || raw.len() > MAX_URL {
        return Err(bad("Give a link to the video."));
    }
    let url = Url::parse(raw).map_err(|_| bad("That is not a valid link."))?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err(bad("Only plain https links can be watched together."));
    }
    // The members' browsers load this address: no private or loopback hosts (the unfurl rules).
    Policy::default()
        .check_url(&url)
        .map_err(|_| bad("That address cannot be used."))?;
    // Literal addresses were checked above; a name must be a public-looking domain (a single label or a
    // local suffix would send every member's browser to their own network).
    if !public_name(&url) {
        return Err(bad("That address cannot be used."));
    }
    Ok(Checked {
        url,
        given: input.title.as_deref().and_then(clean_title),
    })
}

/// Validate and canonicalise `input` without fetching anything. `peertube` are the configured PeerTube instances;
/// a PeerTube link needs the resolver ([`resolve_external`]) and is refused here.
pub fn resolve_with(input: &ExternalInput, peertube: &[String]) -> PartyResult<ExternalMedia> {
    let Checked { url, given } = check(input)?;
    let host = url.host_str().unwrap_or_default().to_owned();

    if let Some(play) = embed::detect_embed(&url, peertube) {
        if !play.sync || play.provider == "peertube" {
            return Err(bad(CANNOT_SHARE));
        }
        debug_assert_eq!(play.mode, PlayMode::Embed);
        let label = if play.provider == "vimeo" {
            "Vimeo"
        } else {
            "YouTube"
        };
        return Ok(ExternalMedia {
            kind: MediaKind::Embed,
            url: play.src,
            provider: play.provider,
            title: given.unwrap_or_else(|| format!("{label} video")),
            format: None,
            page_url: None,
        });
    }
    Ok(ExternalMedia {
        kind: MediaKind::Direct,
        url: url.to_string(),
        provider: "direct".to_owned(),
        title: given.unwrap_or(host),
        format: None,
        page_url: None,
    })
}

/// [`resolve_with`] with the instance's configured PeerTube hosts.
pub fn resolve(input: &ExternalInput) -> PartyResult<ExternalMedia> {
    resolve_with(input, peertube_hosts())
}

/// Whether `input` is a link the resolver would fetch (a listed PeerTube instance), so callers can check the
/// member's rights before any outgoing request.
pub fn fetches(input: &ExternalInput, peertube: Option<&PeerTube>) -> bool {
    let Some(pt) = peertube else { return false };
    Url::parse(input.url.trim())
        .ok()
        .and_then(|u| embed::peertube_video_id(&u, &pt.hosts))
        .is_some()
}

/// Validate `input`; a link to a listed PeerTube instance is resolved through its API when `peertube` is set
/// (without a resolver it is refused, as before wave 50). Everything else is [`resolve_with`].
pub async fn resolve_external(
    input: &ExternalInput,
    peertube: Option<&PeerTube>,
) -> PartyResult<ExternalMedia> {
    let hosts = peertube.map_or(peertube_hosts(), PeerTube::hosts);
    let Checked { url, given } = check(input)?;
    match (peertube, embed::peertube_video_id(&url, hosts)) {
        (Some(pt), Some((host, id))) => {
            resolve_peertube(
                &pt.fetcher,
                &host,
                &id,
                &pt.media_hosts,
                url.as_str(),
                given,
            )
            .await
        }
        _ => resolve_with(input, hosts),
    }
}

// ---- PeerTube (D-516) ------------------------------------------------------------------------------------

/// The fields of `GET /api/v1/videos/<id>` a party needs; everything else in the answer is ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiVideo {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    is_live: bool,
    #[serde(default)]
    privacy: Option<ApiId>,
    #[serde(default)]
    files: Vec<ApiFile>,
    #[serde(default)]
    streaming_playlists: Vec<ApiPlaylist>,
}

#[derive(Debug, Deserialize)]
struct ApiId {
    id: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiFile {
    #[serde(default)]
    resolution: Option<ApiId>,
    #[serde(default)]
    file_url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApiPlaylist {
    #[serde(default)]
    playlist_url: Option<String>,
}

/// PeerTube privacy ids that never play for an anonymous request: private (3), internal (4), password (5).
fn locked_privacy(id: i64) -> bool {
    matches!(id, 3..=5)
}

/// The address a party plays, per D-516: the largest web-video file at or under 1080p, else the first HLS playlist.
fn pick(v: &ApiVideo) -> Option<(&str, MediaFormat)> {
    let file = v
        .files
        .iter()
        .filter_map(|f| {
            let res = f.resolution.as_ref().map_or(0, |r| r.id);
            let url = f.file_url.as_deref().filter(|u| !u.is_empty())?;
            (res <= MAX_RESOLUTION).then_some((res, url))
        })
        .max_by_key(|(res, _)| *res)
        .map(|(_, url)| (url, MediaFormat::File));
    file.or_else(|| {
        v.streaming_playlists
            .iter()
            .find_map(|p| p.playlist_url.as_deref().filter(|u| !u.is_empty()))
            .map(|u| (u, MediaFormat::Hls))
    })
}

/// Whether the browser may be pointed at `chosen`: https, public, on the instance itself or a listed media host.
fn allowed_source(chosen: &str, host: &str, media_hosts: &[String]) -> Option<Url> {
    if chosen.len() > MAX_URL {
        return None;
    }
    let u = Url::parse(chosen).ok()?;
    if u.scheme() != "https" || !u.username().is_empty() || u.password().is_some() {
        return None;
    }
    if u.port().is_some_and(|p| p != 443) || Policy::default().check_url(&u).is_err() {
        return None;
    }
    let h = u.host_str()?.to_ascii_lowercase();
    (h == host || media_hosts.contains(&h)).then_some(u)
}

/// Ask the instance `host` which file or playlist video `id` has, and check it may be played (D-516). `page_url` is
/// the pasted link; `given` the member's title. Each refusal carries the sentence the start dialog shows.
pub async fn resolve_peertube(
    fetcher: &SafeFetcher,
    host: &str,
    id: &str,
    media_hosts: &[String],
    page_url: &str,
    given: Option<String>,
) -> PartyResult<ExternalMedia> {
    let api = Url::parse(&format!("https://{host}/api/v1/videos/{id}"))
        .map_err(|_| bad("That is not a valid link."))?;
    let fetched = match fetcher.fetch(&api, "application/json").await {
        Ok(f) => f,
        Err(FetchError::BadStatus(401 | 403 | 404 | 410)) => return Err(bad(UNAVAILABLE)),
        Err(FetchError::TooLarge | FetchError::UnsupportedType) => return Err(bad(BAD_ANSWER)),
        Err(e) => {
            tracing::info!(host, error = %e, "PeerTube instance did not answer");
            return Err(bad(NO_ANSWER));
        }
    };
    // No redirect off the host: the answer must come from the listed instance itself.
    if fetched.final_url.host_str().map(str::to_ascii_lowercase) != Some(host.to_owned()) {
        return Err(bad(BAD_ANSWER));
    }
    // The fetcher truncates JSON at the cap; a body that reaches it was larger than 256 KB.
    if fetched.class != BodyClass::Json || fetched.body.len() >= MAX_JSON_BYTES {
        return Err(bad(BAD_ANSWER));
    }
    let video: ApiVideo = serde_json::from_slice(&fetched.body).map_err(|_| bad(BAD_ANSWER))?;
    if video.is_live {
        return Err(bad(LIVE));
    }
    if video.privacy.as_ref().is_some_and(|p| locked_privacy(p.id)) {
        return Err(bad(UNAVAILABLE));
    }
    let (chosen, format) = pick(&video).ok_or_else(|| bad(NO_SOURCE))?;
    let source = allowed_source(chosen, host, media_hosts).ok_or_else(|| bad(OFF_LIST_HOST))?;
    let title = given
        .or_else(|| video.name.as_deref().and_then(clean_title))
        .unwrap_or_else(|| "PeerTube video".to_owned());
    Ok(ExternalMedia {
        kind: MediaKind::Direct,
        url: source.to_string(),
        provider: "peertube".to_owned(),
        title,
        format: Some(format),
        page_url: Some(page_url.to_owned()),
    })
}

/// A fetcher that answers `https://<host>/api/v1/videos/<id>` with canned PeerTube answers and refuses everything
/// else. For tests and the debug-build e2e stub (`SR_E2E_PEERTUBE_FIXTURE`); it never touches the network.
#[doc(hidden)]
pub mod stub {
    use std::collections::HashMap;
    use std::net::IpAddr;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use sr_unfurl::SafeFetcher;
    use sr_unfurl::fetch::{FetchError, HopRequest, HopResponse, HttpTransport, Resolver};
    use sr_unfurl::policy::Policy;

    /// Resolves every name to one public address; the canned transport never connects to it.
    pub struct AnyHost;

    #[async_trait]
    impl Resolver for AnyHost {
        async fn resolve(&self, _host: &str, _port: u16) -> Result<Vec<IpAddr>, String> {
            Ok(vec![IpAddr::from([93, 184, 216, 34])])
        }
    }

    type Fallback = Box<dyn Fn(&url::Url) -> Option<HopResponse> + Send + Sync>;

    /// Serves canned responses by exact URL, or through `fallback` for any URL; records what was asked.
    #[derive(Default)]
    pub struct Canned {
        pub routes: HashMap<String, HopResponse>,
        pub fallback: Option<Fallback>,
        pub seen: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl HttpTransport for Canned {
        async fn get(&self, req: &HopRequest) -> Result<HopResponse, FetchError> {
            self.seen
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(req.url.to_string());
            if let Some(r) = self.routes.get(req.url.as_str()) {
                return Ok(r.clone());
            }
            self.fallback
                .as_ref()
                .and_then(|f| f(&req.url))
                .ok_or_else(|| FetchError::Http("no route".into()))
        }
    }

    pub fn json(status: u16, body: impl Into<Vec<u8>>) -> HopResponse {
        HopResponse {
            status,
            content_type: Some("application/json; charset=utf-8".into()),
            location: None,
            body: body.into(),
        }
    }

    /// A fetcher over `transport` with the default (link-preview) policy.
    pub fn fetcher(transport: Arc<Canned>) -> SafeFetcher {
        SafeFetcher::new(Policy::default(), Arc::new(AnyHost), transport)
    }

    /// The e2e instance: any video id answers with one web-video file on the instance itself
    /// (`/static/web-videos/e2e-tone-peertube.wav`, which the e2e specs fulfil in the browser), and an id that
    /// starts with `hls` answers with an HLS playlist only. `policy` is the deployment's unfurl policy.
    pub fn e2e_fetcher(policy: Policy) -> SafeFetcher {
        let transport = Canned {
            fallback: Some(Box::new(|u: &url::Url| {
                let host = u.host_str()?;
                let id = u.path().strip_prefix("/api/v1/videos/")?;
                let body = if id.starts_with("hls") {
                    serde_json::json!({
                        "name": "Stub stream",
                        "isLive": false,
                        "privacy": { "id": 1 },
                        "files": [],
                        "streamingPlaylists": [{
                            "playlistUrl": format!("https://{host}/static/streaming-playlists/hls/{id}/master.m3u8")
                        }],
                    })
                } else {
                    serde_json::json!({
                        "name": "Stub tone",
                        "isLive": false,
                        "privacy": { "id": 1 },
                        "files": [{
                            "resolution": { "id": 480 },
                            "fileUrl": format!("https://{host}/static/web-videos/e2e-tone-peertube.wav")
                        }],
                        "streamingPlaylists": [],
                    })
                };
                Some(json(200, body.to_string()))
            })),
            ..Canned::default()
        };
        SafeFetcher::new(policy, Arc::new(AnyHost), Arc::new(transport))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json as j;
    use std::sync::Arc;
    use stub::{Canned, json};

    fn inp(url: &str, title: Option<&str>) -> ExternalInput {
        ExternalInput {
            url: url.to_owned(),
            title: title.map(str::to_owned),
        }
    }
    fn ok(url: &str) -> ExternalMedia {
        resolve_with(&inp(url, None), &["tube.example.org".to_owned()]).unwrap()
    }
    fn err(url: &str) -> bool {
        resolve_with(&inp(url, None), &["tube.example.org".to_owned()]).is_err()
    }

    #[test]
    fn providers_become_canonical_embeds() {
        let y = ok("https://youtu.be/dQw4w9WgXcQ?t=30");
        assert_eq!(y.kind, MediaKind::Embed);
        assert_eq!(y.provider, "youtube");
        assert_eq!(
            y.url,
            "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ?start=30"
        );
        assert_eq!(y.title, "YouTube video");
        assert_eq!((y.format, y.page_url), (None, None));
        let v = ok("https://vimeo.com/76979871");
        assert_eq!((v.kind, v.provider.as_str()), (MediaKind::Embed, "vimeo"));
        assert_eq!(v.url, "https://player.vimeo.com/video/76979871?dnt=1");
    }

    #[test]
    fn other_https_links_are_direct_files() {
        let d = ok("https://cdn.example.org/media/clip.mp4");
        assert_eq!((d.kind, d.provider.as_str()), (MediaKind::Direct, "direct"));
        assert_eq!(d.title, "cdn.example.org");
        assert_eq!(
            resolve_with(
                &inp("https://cdn.example.org/c.mp3", Some("  Night\u{0}mix  ")),
                &[]
            )
            .unwrap()
            .title,
            "Nightmix"
        );
        let long = "x".repeat(500);
        assert_eq!(
            resolve_with(&inp("https://cdn.example.org/c.mp3", Some(&long)), &[])
                .unwrap()
                .title
                .chars()
                .count(),
            MAX_TITLE
        );
    }

    #[test]
    fn unsafe_addresses_are_refused() {
        for u in [
            "",
            "not a url",
            "http://cdn.example.org/a.mp4",
            "javascript:alert(1)",
            "https://user:pw@cdn.example.org/a.mp4",
            "https://127.0.0.1/a.mp4",
            "https://localhost/a.mp4",
            "https://192.168.1.10/a.mp4",
            "https://[::1]/a.mp4",
            "https://169.254.169.254/latest",
        ] {
            assert!(err(u), "{u}");
        }
        assert!(err(&format!(
            "https://cdn.example.org/{}",
            "a".repeat(2100)
        )));
    }

    #[test]
    fn peertube_needs_the_resolver() {
        assert!(err(
            "https://tube.example.org/w/9c9de5e8-0a1e-484a-b099-e80766180a6d"
        ));
    }

    // ---- the resolver, over a scripted transport ----------------------------------------------------------

    const HOST: &str = "tube.example.org";
    const ID: &str = "9c9de5e8-0a1e-484a-b099-e80766180a6d";
    const API: &str = "https://tube.example.org/api/v1/videos/9c9de5e8-0a1e-484a-b099-e80766180a6d";
    const PAGE: &str = "https://tube.example.org/w/9c9de5e8-0a1e-484a-b099-e80766180a6d";

    fn instance(answer: sr_unfurl::fetch::HopResponse) -> (PeerTube, Arc<Canned>) {
        let mut t = Canned::default();
        t.routes.insert(API.to_owned(), answer);
        let t = Arc::new(t);
        (
            PeerTube::new(
                stub::fetcher(t.clone()),
                vec![HOST.to_owned()],
                vec!["cdn.example.net".to_owned()],
            ),
            t,
        )
    }

    async fn party(answer: sr_unfurl::fetch::HopResponse) -> PartyResult<ExternalMedia> {
        let (pt, _) = instance(answer);
        resolve_external(&inp(PAGE, None), Some(&pt)).await
    }

    fn sentence(r: PartyResult<ExternalMedia>) -> String {
        match r {
            Err(PartyError::Invalid(s)) => s,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    fn video(files: serde_json::Value, playlists: serde_json::Value) -> String {
        j!({
            "id": 7, "uuid": ID, "name": "Night walk", "isLive": false,
            "privacy": { "id": 1, "label": "Public" },
            "files": files, "streamingPlaylists": playlists,
            "description": "ignored", "account": { "name": "x" }
        })
        .to_string()
    }

    fn file(res: i64, url: &str) -> serde_json::Value {
        j!({ "resolution": { "id": res }, "fileUrl": url })
    }

    fn playlist(url: &str) -> serde_json::Value {
        j!({ "playlistUrl": url })
    }

    #[tokio::test]
    async fn a_web_video_file_is_chosen_over_hls() {
        let body = video(
            j!([
                file(360, "https://tube.example.org/static/web-videos/a-360.mp4"),
                file(
                    2160,
                    "https://tube.example.org/static/web-videos/a-2160.mp4"
                ),
                file(
                    1080,
                    "https://tube.example.org/static/web-videos/a-1080.mp4"
                ),
                file(720, "https://tube.example.org/static/web-videos/a-720.mp4"),
            ]),
            j!([playlist(
                "https://tube.example.org/static/streaming-playlists/hls/a/master.m3u8"
            )]),
        );
        let (pt, seen) = instance(json(200, body));
        let m = resolve_external(&inp(PAGE, None), Some(&pt)).await.unwrap();
        assert_eq!(
            (m.kind, m.provider.as_str(), m.format),
            (MediaKind::Direct, "peertube", Some(MediaFormat::File))
        );
        assert_eq!(
            m.url,
            "https://tube.example.org/static/web-videos/a-1080.mp4"
        );
        assert_eq!(m.page_url.as_deref(), Some(PAGE));
        assert_eq!(m.title, "Night walk");
        // One request, to the instance's API, nothing else.
        assert_eq!(*seen.seen.lock().unwrap(), vec![API.to_owned()]);
        // The member's own title wins.
        let given = resolve_external(&inp(PAGE, Some("Ours")), Some(&pt))
            .await
            .unwrap();
        assert_eq!(given.title, "Ours");
    }

    #[tokio::test]
    async fn hls_is_played_when_there_are_no_files() {
        let m = party(json(
            200,
            video(
                j!([]),
                j!([playlist("https://cdn.example.net/hls/a/master.m3u8")]),
            ),
        ))
        .await
        .unwrap();
        assert_eq!(m.format, Some(MediaFormat::Hls));
        assert_eq!(m.url, "https://cdn.example.net/hls/a/master.m3u8");
        // Also from the embed address the chat card hands to Watch together.
        let (pt, _) = instance(json(
            200,
            video(
                j!([]),
                j!([playlist("https://tube.example.org/hls/a/master.m3u8")]),
            ),
        ));
        let e = resolve_external(
            &inp(&format!("https://{HOST}/videos/embed/{ID}"), None),
            Some(&pt),
        )
        .await
        .unwrap();
        assert_eq!(e.format, Some(MediaFormat::Hls));
    }

    #[tokio::test]
    async fn a_live_video_is_refused() {
        let body = j!({
            "name": "Live", "isLive": true, "files": [],
            "streamingPlaylists": [playlist("https://tube.example.org/live/a/master.m3u8")]
        })
        .to_string();
        assert_eq!(sentence(party(json(200, body)).await), LIVE);
    }

    #[tokio::test]
    async fn a_locked_or_missing_video_is_refused() {
        for status in [401, 403, 404] {
            assert_eq!(
                sentence(party(json(status, "{}")).await),
                UNAVAILABLE,
                "{status}"
            );
        }
        let password = j!({
            "isLive": false, "privacy": { "id": 5 },
            "files": [file(720, "https://tube.example.org/a.mp4")]
        })
        .to_string();
        assert_eq!(sentence(party(json(200, password)).await), UNAVAILABLE);
    }

    #[tokio::test]
    async fn a_video_without_a_source_is_refused() {
        let none = video(j!([]), j!([]));
        assert_eq!(sentence(party(json(200, none)).await), NO_SOURCE);
        // Only a file above 1080p and no playlist: nothing a party picks.
        let big = video(j!([file(2160, "https://tube.example.org/a.mp4")]), j!([]));
        assert_eq!(sentence(party(json(200, big)).await), NO_SOURCE);
    }

    #[tokio::test]
    async fn an_off_list_segment_host_is_refused() {
        for addr in [
            "https://objects.elsewhere.example/hls/a/master.m3u8",
            "http://tube.example.org/hls/a/master.m3u8",
            "https://tube.example.org:8443/hls/a/master.m3u8",
            "https://user:pw@tube.example.org/hls/a/master.m3u8",
            "https://127.0.0.1/hls/a/master.m3u8",
            "javascript:alert(1)",
        ] {
            let body = video(j!([]), j!([playlist(addr)]));
            assert_eq!(
                sentence(party(json(200, body)).await),
                OFF_LIST_HOST,
                "{addr}"
            );
        }
        let off = video(
            j!([file(720, "https://s3.elsewhere.example/a.mp4")]),
            j!([]),
        );
        assert_eq!(sentence(party(json(200, off)).await), OFF_LIST_HOST);
    }

    #[tokio::test]
    async fn an_oversize_api_answer_is_refused() {
        let pad = "x".repeat(MAX_JSON_BYTES + 10);
        let body = j!({
            "isLive": false,
            "files": [file(720, "https://tube.example.org/a.mp4")],
            "description": pad
        })
        .to_string();
        assert_eq!(sentence(party(json(200, body)).await), BAD_ANSWER);
        assert_eq!(sentence(party(json(200, "not json")).await), BAD_ANSWER);
        let mut html = json(200, "<html></html>");
        html.content_type = Some("text/html".into());
        assert_eq!(sentence(party(html).await), BAD_ANSWER);
    }

    #[tokio::test]
    async fn a_redirect_off_the_instance_is_refused() {
        let mut t = Canned::default();
        t.routes.insert(
            API.to_owned(),
            sr_unfurl::fetch::HopResponse {
                status: 302,
                location: Some("https://evil.example/api.json".into()),
                ..Default::default()
            },
        );
        t.routes.insert(
            "https://evil.example/api.json".to_owned(),
            json(
                200,
                video(j!([file(720, "https://evil.example/a.mp4")]), j!([])),
            ),
        );
        let pt = PeerTube::new(stub::fetcher(Arc::new(t)), vec![HOST.to_owned()], vec![]);
        assert_eq!(
            sentence(resolve_external(&inp(PAGE, None), Some(&pt)).await),
            BAD_ANSWER
        );
    }

    #[tokio::test]
    async fn unlisted_instances_and_other_links_never_fetch() {
        let (pt, seen) = instance(json(200, "{}"));
        assert!(fetches(&inp(PAGE, None), Some(&pt)));
        assert!(!fetches(&inp(PAGE, None), None));
        // Another PeerTube-shaped host is a direct link, not fetched.
        let other_link = inp("https://other.example.org/w/9c9de5e8-0a1e", None);
        assert!(!fetches(&other_link, Some(&pt)));
        let other = resolve_external(&other_link, Some(&pt)).await.unwrap();
        assert_eq!(other.provider, "direct");
        let yt = resolve_external(&inp("https://youtu.be/dQw4w9WgXcQ", None), Some(&pt))
            .await
            .unwrap();
        assert_eq!(yt.provider, "youtube");
        assert!(seen.seen.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn resolve_peertube_reports_an_instance_that_does_not_answer() {
        let t = Arc::new(Canned::default());
        let r = resolve_peertube(&stub::fetcher(t), HOST, ID, &[], PAGE, None).await;
        assert_eq!(sentence(r), NO_ANSWER);
    }

    #[tokio::test]
    async fn the_e2e_stub_answers_a_file_and_an_hls_only_video() {
        let pt = PeerTube::new(
            stub::e2e_fetcher(Policy::default()),
            vec!["peertube.test".to_owned()],
            vec![],
        );
        let f = resolve_external(&inp("https://peertube.test/w/stubtone01", None), Some(&pt))
            .await
            .unwrap();
        assert_eq!(
            f.url,
            "https://peertube.test/static/web-videos/e2e-tone-peertube.wav"
        );
        let h = resolve_external(&inp("https://peertube.test/w/hlsstub01", None), Some(&pt))
            .await
            .unwrap();
        assert_eq!(h.format, Some(MediaFormat::Hls));
    }
}

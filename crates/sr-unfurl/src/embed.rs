//! Play-here detection (PRF-UNF-001): which links the client may offer to play, and from where.
//!
//! Nothing here fetches anything. A link to YouTube, Vimeo or a configured PeerTube instance maps to
//! the provider's own privacy-enhanced embed address (`youtube-nocookie.com`, Vimeo `dnt=1`); the
//! client loads that address in an iframe only after the reader clicks. A direct media link is
//! classified by the content type the unfurl request saw, never by its extension.
//!
//! No yt-dlp, no proxy, no provider script: the embed is the provider's official page.

use serde::{Deserialize, Serialize};
use url::Url;

/// Environment variable naming the PeerTube instances this deployment embeds (comma separated hosts).
/// The same list feeds the app policy's `frame-src` (`SR_CSP_FRAME` carries the origins, see
/// `apps/web/src/security/csp.ts`), so a PeerTube link is offered only for a host the browser may frame.
pub const PEERTUBE_HOSTS_ENV: &str = "SR_PEERTUBE_HOSTS";

/// Environment variable naming further hosts a member's browser may fetch HLS playlists and segments from
/// (D-517): object storage or a CDN a listed PeerTube instance serves its videos from. Comma separated, parsed like
/// [`PEERTUBE_HOSTS_ENV`], empty by default. The web build reads the same value for the app policy's `connect-src`.
pub const MEDIA_HOSTS_ENV: &str = "SR_MEDIA_HOSTS";

pub const YOUTUBE_EMBED_HOST: &str = "www.youtube-nocookie.com";
pub const VIMEO_EMBED_HOST: &str = "player.vimeo.com";

/// How a client plays a link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayMode {
    /// The provider's privacy-enhanced iframe, loaded after a click.
    Embed,
    /// The browser's own player, loaded from the origin after a click.
    Direct,
}

/// What kind of file a direct link is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectMedia {
    Video,
    Audio,
    /// An HLS playlist (`.m3u8`): native in Safari, otherwise played through the app's HLS loader.
    Hls,
}

/// The "Play here" offer carried by a link preview (`sr_play` on the embed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Play {
    pub mode: PlayMode,
    /// `youtube`, `vimeo`, `peertube`, or `direct`.
    pub provider: String,
    /// The address to load after the click: the privacy-enhanced embed, or the media file itself.
    pub src: String,
    /// Set for `direct` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<DirectMedia>,
    /// Whether Watch Together can drive this player (the provider's `postMessage` commands are known).
    pub sync: bool,
}

/// The configured PeerTube hosts, lower-cased, without ports or paths.
pub fn peertube_hosts_from_env() -> Vec<String> {
    parse_hosts(&std::env::var(PEERTUBE_HOSTS_ENV).unwrap_or_default())
}

/// The extra media hosts ([`MEDIA_HOSTS_ENV`]), lower-cased, without ports or paths.
pub fn media_hosts_from_env() -> Vec<String> {
    parse_hosts(&std::env::var(MEDIA_HOSTS_ENV).unwrap_or_default())
}

/// `a.example, B.example:443` -> `["a.example", "b.example"]`. Entries that are not plain host names are dropped.
pub fn parse_hosts(raw: &str) -> Vec<String> {
    parse_hosts_report(raw).0
}

/// [`parse_hosts`] and the entries it dropped, as written (trimmed). An empty entry between two commas is nothing,
/// not a dropped entry (D-697).
pub fn parse_hosts_report(raw: &str) -> (Vec<String>, Vec<String>) {
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    for entry in raw.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let host = entry
            .trim_start_matches("https://")
            .trim_end_matches('/')
            .split(':')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let plain = !host.is_empty()
            && host.contains('.')
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
        if plain {
            kept.push(host);
        } else {
            dropped.push(entry.to_owned());
        }
    }
    (kept, dropped)
}

/// The warning line for one host variable (`var`, its raw value), or `None` when every entry is a plain host name
/// (D-697). One text for the server's start and for preflight.
pub fn host_warning(var: &str, raw: &str) -> Option<String> {
    let (_, dropped) = parse_hosts_report(raw);
    if dropped.is_empty() {
        return None;
    }
    let entries = dropped
        .iter()
        .map(|e| format!("\"{e}\""))
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!(
        "{var}: {entries} ignored: not a plain host name (write `video.example.org`, no scheme, no path)"
    ))
}

fn is_id(s: &str, len: std::ops::RangeInclusive<usize>) -> bool {
    len.contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// `90`, `1m30s`, `1h2m3s` -> seconds (at most a day).
fn parse_start(raw: &str) -> Option<u32> {
    if raw.is_empty() {
        return None;
    }
    if let Ok(n) = raw.parse::<u32>() {
        return (n <= 86_400).then_some(n);
    }
    let mut total = 0u32;
    let mut num = String::new();
    for ch in raw.chars() {
        match ch {
            '0'..='9' => num.push(ch),
            'h' | 'm' | 's' => {
                let n: u32 = num.parse().ok()?;
                num.clear();
                total += n * match ch {
                    'h' => 3600,
                    'm' => 60,
                    _ => 1,
                };
            }
            _ => return None,
        }
    }
    (num.is_empty() && total <= 86_400).then_some(total)
}

fn query<'a>(url: &'a Url, key: &str) -> Option<std::borrow::Cow<'a, str>> {
    url.query_pairs().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn youtube(url: &Url, host: &str) -> Option<Play> {
    let segs: Vec<&str> = url.path_segments()?.collect();
    let id: String = match host {
        "youtu.be" => (*segs.first()?).to_owned(),
        "youtube.com"
        | "www.youtube.com"
        | "m.youtube.com"
        | "music.youtube.com"
        | "youtube-nocookie.com"
        | "www.youtube-nocookie.com" => match segs.as_slice() {
            ["watch"] => query(url, "v")?.into_owned(),
            ["embed" | "shorts" | "live" | "v", id, ..] => (*id).to_owned(),
            _ => return None,
        },
        _ => return None,
    };
    if !is_id(&id, 11..=11) {
        return None;
    }
    let start = query(url, "t")
        .or_else(|| query(url, "start"))
        .and_then(|t| parse_start(&t));
    let mut src = format!("https://{YOUTUBE_EMBED_HOST}/embed/{id}");
    if let Some(s) = start.filter(|s| *s > 0) {
        src.push_str(&format!("?start={s}"));
    }
    Some(Play {
        mode: PlayMode::Embed,
        provider: "youtube".into(),
        src,
        media: None,
        sync: true,
    })
}

fn vimeo(url: &Url, host: &str) -> Option<Play> {
    if !matches!(host, "vimeo.com" | "www.vimeo.com" | "player.vimeo.com") {
        return None;
    }
    let segs: Vec<&str> = url.path_segments()?.collect();
    let numeric =
        |s: &&str| !s.is_empty() && s.len() <= 12 && s.bytes().all(|b| b.is_ascii_digit());
    // vimeo.com/123, vimeo.com/123/abcdef0123 (unlisted), vimeo.com/channels/x/123,
    // vimeo.com/groups/x/videos/123, player.vimeo.com/video/123
    let (id, hash) = match segs.as_slice() {
        ["video", id, ..] if host == "player.vimeo.com" => {
            (*id, query(url, "h").map(|h| h.into_owned()))
        }
        [id] if numeric(id) => (*id, None),
        [id, h] if numeric(id) => (*id, Some((*h).to_owned())),
        ["channels", _, id] | ["groups", _, "videos", id] if numeric(id) => (*id, None),
        _ => return None,
    };
    if !numeric(&id) || hash.as_deref().is_some_and(|h| !is_id(h, 6..=24)) {
        return None;
    }
    let mut src = format!("https://{VIMEO_EMBED_HOST}/video/{id}?dnt=1");
    if let Some(h) = hash {
        src.push_str(&format!("&h={h}"));
    }
    Some(Play {
        mode: PlayMode::Embed,
        provider: "vimeo".into(),
        src,
        media: None,
        sync: true,
    })
}

/// The host and video id of a link to a configured PeerTube instance (`/w/<id>`, `/videos/watch/<id>`,
/// `/videos/embed/<id>`), for the party path (D-516). Only plain https links on port 443 qualify.
pub fn peertube_video_id(url: &Url, hosts: &[String]) -> Option<(String, String)> {
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    if url.port().is_some_and(|p| p != 443) {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    if !hosts.contains(&host) {
        return None;
    }
    let segs: Vec<&str> = url.path_segments()?.collect();
    let id = match segs.as_slice() {
        ["w", id] | ["videos", "watch", id] | ["videos", "embed", id] => *id,
        _ => return None,
    };
    is_id(id, 8..=40).then(|| (host, id.to_owned()))
}

fn peertube(url: &Url, hosts: &[String]) -> Option<Play> {
    let (host, id) = peertube_video_id(url, hosts)?;
    Some(Play {
        mode: PlayMode::Embed,
        provider: "peertube".into(),
        src: format!("https://{host}/videos/embed/{id}"),
        media: None,
        // D-515: a party resolves the video through the instance's API and plays the file itself.
        sync: true,
    })
}

/// The privacy-enhanced embed for a link, when it is one of the providers. `peertube_hosts` are the
/// configured instances ([`peertube_hosts_from_env`]). Only `https` links qualify.
pub fn detect_embed(url: &Url, peertube_hosts: &[String]) -> Option<Play> {
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    if url.port().is_some_and(|p| p != 443) {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    youtube(url, &host)
        .or_else(|| vimeo(url, &host))
        .or_else(|| peertube(url, peertube_hosts))
}

/// The direct-media kind of a content type, `None` for anything else. HLS playlists have several
/// registered and de-facto types; they are checked before the generic `audio/` family.
pub fn direct_media_for(content_type: Option<&str>) -> Option<DirectMedia> {
    let ct = content_type?.split(';').next()?.trim().to_ascii_lowercase();
    match ct.as_str() {
        "application/vnd.apple.mpegurl"
        | "application/x-mpegurl"
        | "application/mpegurl"
        | "audio/mpegurl"
        | "audio/x-mpegurl" => Some(DirectMedia::Hls),
        "video/mp4" | "video/webm" | "video/ogg" | "video/quicktime" => Some(DirectMedia::Video),
        "audio/mpeg" | "audio/mp3" | "audio/ogg" | "audio/opus" | "audio/webm" | "audio/aac"
        | "audio/mp4" | "audio/x-m4a" | "audio/wav" | "audio/x-wav" | "audio/flac"
        | "application/ogg" => Some(DirectMedia::Audio),
        _ => None,
    }
}

/// The "Play here" offer for a direct file the unfurl request saw with `content_type`.
pub fn direct_play(url: &Url, content_type: Option<&str>) -> Option<Play> {
    if url.scheme() != "https" {
        return None;
    }
    let media = direct_media_for(content_type)?;
    Some(Play {
        mode: PlayMode::Direct,
        provider: "direct".into(),
        src: url.to_string(),
        media: Some(media),
        sync: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(raw: &str) -> Option<Play> {
        detect_embed(&Url::parse(raw).unwrap(), &["tube.example.org".to_owned()])
    }
    fn src(raw: &str) -> String {
        d(raw).unwrap_or_else(|| panic!("no embed for {raw}")).src
    }

    #[test]
    fn youtube_links_become_nocookie_embeds() {
        let want = "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ";
        for raw in [
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/watch?v=dQw4w9WgXcQ&feature=share",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            "https://www.youtube.com/embed/dQw4w9WgXcQ",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
            "https://www.youtube.com/live/dQw4w9WgXcQ",
            "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ",
        ] {
            assert_eq!(src(raw), want, "{raw}");
        }
        let p = d("https://youtu.be/dQw4w9WgXcQ").unwrap();
        assert_eq!(
            (p.mode, p.provider.as_str(), p.sync),
            (PlayMode::Embed, "youtube", true)
        );
    }

    #[test]
    fn youtube_start_times() {
        assert!(src("https://youtu.be/dQw4w9WgXcQ?t=90").ends_with("?start=90"));
        assert!(src("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1m30s").ends_with("?start=90"));
        assert!(
            src("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1h0m1s").ends_with("?start=3601")
        );
        assert!(!src("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=bogus").contains('?'));
        assert!(!src("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=0").contains('?'));
    }

    #[test]
    fn youtube_rejects_what_is_not_a_video() {
        for raw in [
            "https://www.youtube.com/",
            "https://www.youtube.com/watch",
            "https://www.youtube.com/watch?v=short",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ%22%3E%3Cscript",
            "https://www.youtube.com/@channel",
            "https://youtube.com.evil.example/watch?v=dQw4w9WgXcQ",
            "https://evilyoutube.com/watch?v=dQw4w9WgXcQ",
            "http://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://user:pw@www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://www.youtube.com:8443/watch?v=dQw4w9WgXcQ",
        ] {
            assert!(d(raw).is_none(), "{raw}");
        }
    }

    #[test]
    fn vimeo_links_carry_do_not_track() {
        for raw in [
            "https://vimeo.com/76979871",
            "https://www.vimeo.com/76979871",
            "https://vimeo.com/channels/staffpicks/76979871",
            "https://vimeo.com/groups/x/videos/76979871",
            "https://player.vimeo.com/video/76979871",
        ] {
            assert_eq!(
                src(raw),
                "https://player.vimeo.com/video/76979871?dnt=1",
                "{raw}"
            );
        }
        assert_eq!(
            src("https://vimeo.com/76979871/abcdef0123"),
            "https://player.vimeo.com/video/76979871?dnt=1&h=abcdef0123"
        );
        assert_eq!(
            src("https://player.vimeo.com/video/76979871?h=abcdef0123&autoplay=1"),
            "https://player.vimeo.com/video/76979871?dnt=1&h=abcdef0123"
        );
        assert!(d("https://vimeo.com/").is_none());
        assert!(d("https://vimeo.com/staff").is_none());
        assert!(d("https://vimeo.com/76979871/not a hash!").is_none());
    }

    #[test]
    fn peertube_only_for_configured_hosts() {
        assert_eq!(
            src("https://tube.example.org/w/9c9de5e8-0a1e-484a-b099-e80766180a6d"),
            "https://tube.example.org/videos/embed/9c9de5e8-0a1e-484a-b099-e80766180a6d"
        );
        assert_eq!(
            src("https://tube.example.org/videos/watch/9c9de5e8-0a1e-484a-b099-e80766180a6d"),
            "https://tube.example.org/videos/embed/9c9de5e8-0a1e-484a-b099-e80766180a6d"
        );
        let p = d("https://tube.example.org/w/9c9de5e8").unwrap();
        // D-515: Watch together is offered; the party resolves the video through the instance's API.
        assert!(p.sync);
        assert!(d("https://other.example.org/w/9c9de5e8-0a1e").is_none());
        assert!(d("https://tube.example.org/about").is_none());
        // No configured instance at all: nothing is a PeerTube link.
        let u = Url::parse("https://tube.example.org/w/9c9de5e8-0a1e").unwrap();
        assert!(detect_embed(&u, &[]).is_none());
    }

    #[test]
    fn peertube_video_ids_for_the_party_path() {
        let hosts = ["tube.example.org".to_owned()];
        let id = |raw: &str| peertube_video_id(&Url::parse(raw).unwrap(), &hosts);
        let want = Some((
            "tube.example.org".to_owned(),
            "9c9de5e8-0a1e-484a-b099-e80766180a6d".to_owned(),
        ));
        for raw in [
            "https://tube.example.org/w/9c9de5e8-0a1e-484a-b099-e80766180a6d",
            "https://Tube.Example.org/videos/watch/9c9de5e8-0a1e-484a-b099-e80766180a6d",
            "https://tube.example.org/videos/embed/9c9de5e8-0a1e-484a-b099-e80766180a6d",
            "https://tube.example.org:443/w/9c9de5e8-0a1e-484a-b099-e80766180a6d?start=10",
        ] {
            assert_eq!(id(raw), want, "{raw}");
        }
        for raw in [
            "http://tube.example.org/w/9c9de5e8-0a1e",
            "https://tube.example.org:8443/w/9c9de5e8-0a1e",
            "https://u:p@tube.example.org/w/9c9de5e8-0a1e",
            "https://other.example.org/w/9c9de5e8-0a1e",
            "https://tube.example.org/w/short",
            "https://tube.example.org/w/9c9de5e8-0a1e/extra",
            "https://tube.example.org/api/v1/videos/9c9de5e8-0a1e",
            "https://tube.example.org/w/9c9de5e8%2F..%2Fx",
        ] {
            assert_eq!(id(raw), None, "{raw}");
        }
        let u = Url::parse("https://tube.example.org/w/9c9de5e8-0a1e").unwrap();
        assert_eq!(peertube_video_id(&u, &[]), None);
    }

    #[test]
    fn the_media_hosts_list_is_its_own_variable_parsed_like_the_peertube_one() {
        assert_eq!(MEDIA_HOSTS_ENV, "SR_MEDIA_HOSTS");
        assert_ne!(MEDIA_HOSTS_ENV, PEERTUBE_HOSTS_ENV);
        // Unset in the test environment: nothing is listed.
        if std::env::var_os(MEDIA_HOSTS_ENV).is_none() {
            assert!(media_hosts_from_env().is_empty());
        }
        assert_eq!(
            parse_hosts("https://cdn.Example.net/, segments.example.org:443"),
            vec!["cdn.example.net", "segments.example.org"]
        );
    }

    #[test]
    fn host_list_parsing() {
        assert_eq!(
            parse_hosts(
                " Tube.Example.org , https://a.example:443/ ,,bad host,localhost,x_y.example"
            ),
            vec!["tube.example.org", "a.example"]
        );
    }

    #[test]
    fn dropped_host_entries_are_reported_as_written() {
        let (kept, dropped) = parse_hosts_report(
            " Tube.Example.org , https://a.example:443/ ,,bad host,localhost,x_y.example, http://host.example,host.example/path,*.example.org,",
        );
        assert_eq!(kept, vec!["tube.example.org", "a.example"]);
        assert_eq!(
            dropped,
            vec![
                "bad host",
                "localhost",
                "x_y.example",
                "http://host.example",
                "host.example/path",
                "*.example.org"
            ]
        );
        assert_eq!(parse_hosts_report(""), (vec![], vec![]));
        assert_eq!(parse_hosts_report(" , ,"), (vec![], vec![]));
        let raw = "a.example,nodot,b.example";
        assert_eq!(parse_hosts(raw), parse_hosts_report(raw).0);
    }

    #[test]
    fn direct_media_is_decided_by_content_type() {
        let u = Url::parse("https://cdn.example.org/clip").unwrap();
        for (ct, want) in [
            ("video/mp4", DirectMedia::Video),
            ("video/webm; codecs=vp9", DirectMedia::Video),
            ("audio/mpeg", DirectMedia::Audio),
            ("audio/ogg", DirectMedia::Audio),
            ("AUDIO/MP3", DirectMedia::Audio),
            ("application/vnd.apple.mpegurl", DirectMedia::Hls),
            ("application/x-mpegURL", DirectMedia::Hls),
            ("audio/mpegurl", DirectMedia::Hls),
        ] {
            let p = direct_play(&u, Some(ct)).unwrap_or_else(|| panic!("{ct}"));
            assert_eq!(
                (p.mode, p.media, p.sync),
                (PlayMode::Direct, Some(want), true),
                "{ct}"
            );
            assert_eq!(p.src, "https://cdn.example.org/clip");
        }
        assert!(direct_play(&u, Some("text/html")).is_none());
        assert!(direct_play(&u, Some("application/octet-stream")).is_none());
        assert!(direct_play(&u, None).is_none());
        // Plain http is not offered: the app policy allows https media only.
        let plain = Url::parse("http://cdn.example.org/clip.mp4").unwrap();
        assert!(direct_play(&plain, Some("video/mp4")).is_none());
    }
}

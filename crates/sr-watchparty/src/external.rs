//! Media that is not in the Cinema catalog (PRF-UNF-001): a provider's privacy-enhanced embed or a direct file.
//!
//! The server fetches nothing and loads no provider script. It only decides whether the address is one a
//! member's browser may be sent to, and stores the canonical form. Each member's browser loads it after a
//! click, and the iframe's own `postMessage` commands (YouTube, Vimeo) or the native player carry the sync.

use std::sync::OnceLock;

use sr_unfurl::embed::{self, PlayMode};
use sr_unfurl::policy::Policy;
use url::Url;

use crate::error::{PartyError, PartyResult};
use crate::model::{ExternalInput, MediaKind};

/// What the party stores for external media.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalMedia {
    pub kind: MediaKind,
    /// The embed address (`https://www.youtube-nocookie.com/embed/ID`) or the media file.
    pub url: String,
    /// `youtube`, `vimeo` or `direct`.
    pub provider: String,
    pub title: String,
}

const MAX_URL: usize = 2048;
const MAX_TITLE: usize = 120;

fn peertube_hosts() -> &'static [String] {
    static HOSTS: OnceLock<Vec<String>> = OnceLock::new();
    HOSTS.get_or_init(embed::peertube_hosts_from_env)
}

fn bad(msg: &str) -> PartyError {
    PartyError::Invalid(msg.to_owned())
}

/// Validate and canonicalise `input`. `peertube` are the configured PeerTube instances.
pub fn resolve_with(input: &ExternalInput, peertube: &[String]) -> PartyResult<ExternalMedia> {
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
    if let Some(url::Host::Domain(d)) = url.host() {
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
        if !d.contains('.') || local {
            return Err(bad("That address cannot be used."));
        }
    }

    let given = input
        .title
        .as_deref()
        .map(|t| t.chars().filter(|c| !c.is_control()).collect::<String>())
        .map(|t| t.trim().chars().take(MAX_TITLE).collect::<String>())
        .filter(|t| !t.is_empty());
    let host = url.host_str().unwrap_or_default().to_owned();

    if let Some(play) = embed::detect_embed(&url, peertube) {
        if !play.sync {
            return Err(bad(
                "That player cannot be shared yet. Open it in a message to play it for yourself.",
            ));
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
        });
    }
    Ok(ExternalMedia {
        kind: MediaKind::Direct,
        url: url.to_string(),
        provider: "direct".to_owned(),
        title: given.unwrap_or(host),
    })
}

/// [`resolve_with`] with the instance's configured PeerTube hosts.
pub fn resolve(input: &ExternalInput) -> PartyResult<ExternalMedia> {
    resolve_with(input, peertube_hosts())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn peertube_cannot_be_driven_yet() {
        assert!(err(
            "https://tube.example.org/w/9c9de5e8-0a1e-484a-b099-e80766180a6d"
        ));
    }
}

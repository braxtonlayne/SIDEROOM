//! Turning a URL into a preview embed: fetch, parse, build, cache, rate limit.

use crate::embed::{self, Play};
use crate::fetch::{BodyClass, FetchError, Fetched, SafeFetcher};
use crate::parse::{self, OEmbed, PageMeta, clean_text};
use moka::future::Cache;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};
use url::Url;

const ACCEPT_PAGE: &str = "text/html,application/xhtml+xml,image/*,video/*;q=0.8";
const ACCEPT_JSON: &str = "application/json";

/// How long a successful preview (or a page with nothing to preview) is reused.
pub const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
/// How long a failed fetch is remembered, so a dead link is not retried per message.
pub const FAILURE_TTL: Duration = Duration::from_secs(10 * 60);

/// A preview in Discord's embed JSON shape.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UnfurledEmbed {
    /// `link`, `image`, `video` or `rich`.
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "is_zero")]
    pub color: u32,
    pub provider: Provider,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<Author>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<Media>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<Media>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<Media>,
    /// "Play here" (PRF-UNF-001): the privacy-enhanced embed or the direct file, loaded only after a click.
    #[serde(rename = "sr_play", skip_serializing_if = "Option::is_none")]
    pub play: Option<Play>,
}

fn is_zero(v: &u32) -> bool {
    *v == 0
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Provider {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Author {
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Media {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

/// Why no preview was produced.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UnfurlError {
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error("too many requests to {0}")]
    RateLimited(String),
}

/// Sliding one-minute window per host.
pub struct HostLimiter {
    per_minute: AtomicU32,
    hosts: Mutex<HashMap<String, (Instant, u32)>>,
}

impl HostLimiter {
    const WINDOW: Duration = Duration::from_secs(60);
    const MAX_HOSTS: usize = 10_000;

    pub fn new(per_minute: u32) -> Self {
        Self {
            per_minute: AtomicU32::new(per_minute),
            hosts: Mutex::new(HashMap::new()),
        }
    }

    /// Change the budget; applies from the next request (D-300).
    pub fn set_per_minute(&self, per_minute: u32) {
        self.per_minute.store(per_minute, Ordering::Relaxed);
    }

    /// Count one request to `host` at `now`; false when the host is over its budget.
    pub fn allow(&self, host: &str, now: Instant) -> bool {
        let mut map = self.hosts.lock().unwrap_or_else(|p| p.into_inner());
        if map.len() >= Self::MAX_HOSTS {
            map.retain(|_, (start, _)| now.saturating_duration_since(*start) < Self::WINDOW);
        }
        let e = map.entry(host.to_ascii_lowercase()).or_insert((now, 0));
        if now.saturating_duration_since(e.0) >= Self::WINDOW {
            *e = (now, 0);
        }
        if e.1 >= self.per_minute.load(Ordering::Relaxed) {
            return false;
        }
        e.1 += 1;
        true
    }
}

pub struct Unfurler {
    fetcher: SafeFetcher,
    limiter: HostLimiter,
    ok: Cache<String, Option<UnfurledEmbed>>,
    failed: Cache<String, ()>,
    /// Configured PeerTube instances (`SR_PEERTUBE_HOSTS`).
    peertube: Vec<String>,
}

impl Unfurler {
    /// `per_host_per_minute` bounds outgoing requests to any single host.
    pub fn new(fetcher: SafeFetcher, per_host_per_minute: u32) -> Self {
        Self {
            fetcher,
            limiter: HostLimiter::new(per_host_per_minute),
            ok: Cache::builder()
                .max_capacity(20_000)
                .time_to_live(CACHE_TTL)
                .build(),
            failed: Cache::builder()
                .max_capacity(20_000)
                .time_to_live(FAILURE_TTL)
                .build(),
            peertube: embed::peertube_hosts_from_env(),
        }
    }

    /// The PeerTube instances whose links get a "Play here" offer (default: `SR_PEERTUBE_HOSTS`).
    #[must_use]
    pub fn with_peertube_hosts(mut self, hosts: Vec<String>) -> Self {
        self.peertube = hosts;
        self
    }

    /// Change the per-host request budget while running (D-300).
    pub fn set_per_host_per_minute(&self, per_minute: u32) {
        self.limiter.set_per_minute(per_minute.max(1));
    }

    /// Whether the operator's deny list or the URL rules refuse this link outright.
    pub fn parse_link(&self, raw: &str) -> Option<Url> {
        let url = Url::parse(raw.trim()).ok()?;
        self.fetcher.policy().check_url(&url).ok()?;
        Some(url)
    }

    /// The preview for `raw`, cached for 24 hours. `Ok(None)` means the page was
    /// fetched but has nothing worth showing.
    pub async fn unfurl(&self, raw: &str) -> Result<Option<UnfurledEmbed>, UnfurlError> {
        let url = Url::parse(raw.trim()).map_err(|e| FetchError::BadUrl(e.to_string()))?;
        let key = url.as_str().to_owned();
        // The policy comes first, even for a cached preview: a host the operator just denied is
        // refused on the next request (D-300), and no rate budget is spent on it.
        self.fetcher
            .policy()
            .check_url(&url)
            .map_err(FetchError::from)?;
        if let Some(hit) = self.ok.get(&key).await {
            return Ok(hit);
        }
        if self.failed.get(&key).await.is_some() {
            return Err(FetchError::Http("recently failed".into()).into());
        }
        let host = url.host_str().unwrap_or_default().to_owned();
        if !self.limiter.allow(&host, Instant::now()) {
            return Err(UnfurlError::RateLimited(host));
        }
        match self.build(&url).await {
            Ok(embed) => {
                self.ok.insert(key, embed.clone()).await;
                Ok(embed)
            }
            Err(e) => {
                self.failed.insert(key, ()).await;
                Err(e.into())
            }
        }
    }

    async fn build(&self, url: &Url) -> Result<Option<UnfurledEmbed>, FetchError> {
        // A provider link is recognised from the address alone, so the card still offers "Play here"
        // when the provider's page cannot be read (consent walls, bot checks).
        let provider = embed::detect_embed(url, &self.peertube);
        let page = match self.fetcher.fetch(url, ACCEPT_PAGE).await {
            Ok(page) => page,
            Err(_) if provider.is_some() => return Ok(provider.map(|p| provider_card(url, p))),
            Err(e) => return Err(e),
        };
        match page.class {
            BodyClass::Image => Ok(Some(image_embed(&page))),
            BodyClass::Video => Ok(Some(video_embed(&page))),
            BodyClass::Audio => Ok(Some(audio_embed(&page))),
            BodyClass::Json => Ok(None),
            BodyClass::Html => {
                let html = parse::decode_html(&page.body, page.content_type.as_deref());
                let meta = parse::parse_html(&html, &page.final_url);
                let oembed = self.oembed_for(&meta, &page.final_url).await;
                let card = page_embed(
                    &meta,
                    oembed.as_ref(),
                    &page.final_url,
                    self.fetcher.policy(),
                );
                // A redirect may have landed on another address; the offer follows the final one.
                let provider = embed::detect_embed(&page.final_url, &self.peertube).or(provider);
                Ok(match (card, provider) {
                    (Some(mut card), Some(play)) => {
                        // The provider's own player replaces the page's `og:video` iframe address.
                        card.video = None;
                        card.kind = "link".into();
                        card.play = Some(play);
                        Some(card)
                    }
                    (None, Some(play)) => Some(provider_card(url, play)),
                    (card, None) => card,
                })
            }
        }
    }

    /// oEmbed discovery (JSON only), only when the page itself is thin.
    async fn oembed_for(&self, meta: &PageMeta, page_url: &Url) -> Option<OEmbed> {
        let href = meta.oembed_json.as_deref()?;
        if meta.title.is_some() && meta.image.is_some() {
            return None;
        }
        let url = Url::parse(href).ok()?;
        let host = url.host_str()?.to_owned();
        if !self.limiter.allow(&host, Instant::now()) {
            return None;
        }
        let doc = self.fetcher.fetch(&url, ACCEPT_JSON).await.ok()?;
        if doc.class != BodyClass::Json {
            return None;
        }
        parse::parse_oembed(&doc.body, page_url)
    }
}

fn host_of(url: &Url) -> String {
    url.host_str().unwrap_or("").to_owned()
}

fn origin_of(url: &Url) -> Option<String> {
    let host = url.host_str()?;
    Some(format!("{}://{host}", url.scheme()))
}

fn provider_for(url: &Url, name: Option<&str>) -> Provider {
    Provider {
        name: name.map_or_else(|| host_of(url), str::to_owned),
        url: origin_of(url),
    }
}

/// A direct link to an image: an `image` embed whose thumbnail carries the size.
pub fn image_embed(page: &Fetched) -> UnfurledEmbed {
    let dims = imagesize::blob_size(&page.body).ok();
    let (width, height) = dims.map_or((None, None), |d| {
        (u32::try_from(d.width).ok(), u32::try_from(d.height).ok())
    });
    UnfurledEmbed {
        kind: "image".into(),
        url: page.final_url.to_string(),
        provider: provider_for(&page.final_url, None),
        thumbnail: Some(Media {
            url: page.final_url.to_string(),
            width,
            height,
        }),
        ..UnfurledEmbed::default()
    }
}

/// A direct link to a video file.
pub fn video_embed(page: &Fetched) -> UnfurledEmbed {
    UnfurledEmbed {
        kind: "video".into(),
        url: page.final_url.to_string(),
        provider: provider_for(&page.final_url, None),
        video: Some(Media {
            url: page.final_url.to_string(),
            ..Media::default()
        }),
        play: embed::direct_play(&page.final_url, page.content_type.as_deref()),
        ..UnfurledEmbed::default()
    }
}

/// A direct link to an audio file or an HLS playlist (the content type decides which).
pub fn audio_embed(page: &Fetched) -> UnfurledEmbed {
    let play = embed::direct_play(&page.final_url, page.content_type.as_deref());
    let hls = play
        .as_ref()
        .is_some_and(|p| p.media == Some(embed::DirectMedia::Hls));
    UnfurledEmbed {
        kind: if hls { "video" } else { "audio" }.into(),
        url: page.final_url.to_string(),
        provider: provider_for(&page.final_url, None),
        play,
        ..UnfurledEmbed::default()
    }
}

/// The card of a provider link whose page gave nothing to show: the host and the play offer.
fn provider_card(url: &Url, play: Play) -> UnfurledEmbed {
    let name = match play.provider.as_str() {
        "youtube" => "YouTube".to_owned(),
        "vimeo" => "Vimeo".to_owned(),
        _ => host_of(url),
    };
    UnfurledEmbed {
        kind: "link".into(),
        url: url.to_string(),
        provider: Provider {
            name,
            url: origin_of(url),
        },
        play: Some(play),
        ..UnfurledEmbed::default()
    }
}

/// Build the preview of an HTML page. `None` when there is nothing to show.
pub fn page_embed(
    meta: &PageMeta,
    oembed: Option<&OEmbed>,
    page_url: &Url,
    policy: &crate::policy::Policy,
) -> Option<UnfurledEmbed> {
    let safe = |u: &Option<String>| {
        u.as_deref()
            .filter(|s| Url::parse(s).is_ok_and(|u| policy.check_url(&u).is_ok()))
            .map(str::to_owned)
    };
    let title = meta
        .title
        .clone()
        .or_else(|| oembed.and_then(|o| o.title.clone()));
    let description = meta.description.clone();
    let site = meta
        .site_name
        .clone()
        .or_else(|| oembed.and_then(|o| o.provider_name.clone()));
    let image = safe(&meta.image);
    let oembed_thumb = oembed.and_then(|o| {
        safe(&o.thumbnail_url).map(|url| Media {
            url,
            width: o.thumbnail_width,
            height: o.thumbnail_height,
        })
    });
    let video = safe(&meta.video);
    if title.is_none() && description.is_none() && image.is_none() && oembed_thumb.is_none() {
        return None;
    }

    let og_type = meta.og_type.as_deref().unwrap_or("");
    let is_video = (og_type.starts_with("video") && video.is_some())
        || oembed.is_some_and(|o| o.kind.as_deref() == Some("video"));
    let kind = if is_video && video.is_some() {
        "video"
    } else if oembed.is_some_and(|o| matches!(o.kind.as_deref(), Some("rich" | "video"))) {
        "rich"
    } else {
        "link"
    };

    let picture = image
        .map(|url| Media {
            url,
            width: meta.image_width,
            height: meta.image_height,
        })
        .or(oembed_thumb);
    let large = meta.twitter_card.as_deref() == Some("summary_large_image");
    let (thumbnail, big_image) = match picture {
        Some(p) if large => (None, Some(p)),
        other => (other, None),
    };

    let provider_url = oembed
        .and_then(|o| o.provider_url.clone())
        .or_else(|| origin_of(page_url));
    Some(UnfurledEmbed {
        kind: kind.into(),
        url: page_url.to_string(),
        title,
        description,
        color: meta.theme_color.unwrap_or(0),
        provider: Provider {
            name: site.unwrap_or_else(|| host_of(page_url)),
            url: provider_url,
        },
        author: oembed
            .and_then(|o| o.author_name.as_deref())
            .and_then(|a| clean_text(a, 256))
            .map(|name| Author { name }),
        thumbnail,
        image: big_image,
        video: if kind == "video" {
            video.map(|url| Media {
                url,
                width: meta.video_width,
                height: meta.video_height,
            })
        } else {
            None
        },
        play: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::testing::{ScriptedTransport, TableResolver, html, ip};
    use crate::fetch::{HopResponse, Resolver};
    use crate::policy::Policy;
    use std::sync::Arc;

    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0,
        0, 0, 0x20, 0, 0, 0, 0x10, 8, 6, 0, 0, 0,
    ];

    fn unfurler(
        routes: Vec<(&str, HopResponse)>,
        per_min: u32,
    ) -> (Unfurler, Arc<ScriptedTransport>) {
        let mut r = TableResolver::default();
        r.0.insert("site.test".into(), vec![ip("93.184.216.34")]);
        r.0.insert("img.test".into(), vec![ip("93.184.216.35")]);
        r.0.insert("www.youtube.com".into(), vec![ip("93.184.216.40")]);
        r.0.insert("vimeo.com".into(), vec![ip("93.184.216.41")]);
        r.0.insert("media.test".into(), vec![ip("93.184.216.42")]);
        let t = Arc::new(ScriptedTransport {
            routes: routes.into_iter().map(|(u, r)| (u.to_owned(), r)).collect(),
            ..ScriptedTransport::default()
        });
        let resolver: Arc<dyn Resolver> = Arc::new(r);
        let f = SafeFetcher::new(Policy::default(), resolver, t.clone());
        (Unfurler::new(f, per_min), t)
    }

    #[tokio::test]
    async fn page_becomes_a_link_embed() {
        let page = html(
            r##"<head><meta property="og:title" content="Hello">
            <meta property="og:description" content="World">
            <meta property="og:site_name" content="Site">
            <meta property="og:image" content="http://img.test/a.png">
            <meta name="theme-color" content="#102030"></head>"##,
        );
        let (u, _) = unfurler(vec![("http://site.test/p", page)], 10);
        let e = u.unfurl("http://site.test/p").await.unwrap().unwrap();
        assert_eq!(e.kind, "link");
        assert_eq!(e.title.as_deref(), Some("Hello"));
        assert_eq!(e.provider.name, "Site");
        assert_eq!(e.color, 0x102030);
        assert_eq!(e.thumbnail.as_ref().unwrap().url, "http://img.test/a.png");
        let j = serde_json::to_value(&e).unwrap();
        assert_eq!(j["type"], "link");
        assert!(j.get("image").is_none());
    }

    #[tokio::test]
    async fn direct_image_and_video_links() {
        let img = HopResponse {
            status: 200,
            content_type: Some("image/png".into()),
            body: PNG_1X1.to_vec(),
            ..HopResponse::default()
        };
        let vid = HopResponse {
            status: 200,
            content_type: Some("video/mp4".into()),
            ..HopResponse::default()
        };
        let (u, _) = unfurler(
            vec![
                ("http://img.test/a.png", img),
                ("http://img.test/v.mp4", vid),
            ],
            10,
        );
        let e = u.unfurl("http://img.test/a.png").await.unwrap().unwrap();
        assert_eq!(e.kind, "image");
        let t = e.thumbnail.unwrap();
        assert_eq!((t.width, t.height), (Some(32), Some(16)));
        let v = u.unfurl("http://img.test/v.mp4").await.unwrap().unwrap();
        assert_eq!(v.kind, "video");
        assert_eq!(v.video.unwrap().url, "http://img.test/v.mp4");
    }

    #[tokio::test]
    async fn results_are_cached_and_rate_limited_per_host() {
        let page = html(r#"<head><title>One</title></head>"#);
        let (u, t) = unfurler(
            vec![
                ("http://site.test/a", page.clone()),
                ("http://site.test/b", page.clone()),
                ("http://site.test/c", page),
            ],
            2,
        );
        u.unfurl("http://site.test/a").await.unwrap();
        u.unfurl("http://site.test/a").await.unwrap(); // cache hit, no budget used
        assert_eq!(t.seen.lock().unwrap().len(), 1);
        u.unfurl("http://site.test/b").await.unwrap();
        assert!(matches!(
            u.unfurl("http://site.test/c").await,
            Err(UnfurlError::RateLimited(_))
        ));
    }

    #[tokio::test]
    async fn a_host_denied_later_is_refused_on_the_next_request_even_when_cached() {
        let page = html(r#"<head><title>One</title></head>"#);
        let live = sr_net::Live::new(sr_net::HostLists::default());
        let mut r = TableResolver::default();
        r.0.insert("site.test".into(), vec![ip("93.184.216.34")]);
        let t = Arc::new(ScriptedTransport {
            routes: [("http://site.test/a".to_owned(), page)]
                .into_iter()
                .collect(),
            ..ScriptedTransport::default()
        });
        let policy = Policy {
            live_hosts: Some(live.clone()),
            ..Policy::default()
        };
        let resolver: Arc<dyn Resolver> = Arc::new(r);
        let u = Unfurler::new(SafeFetcher::new(policy, resolver, t), 10);
        assert!(u.unfurl("http://site.test/a").await.unwrap().is_some());
        live.store(sr_net::HostLists {
            deny_hosts: vec!["site.test".into()],
            allow_hosts: vec![],
        });
        assert!(u.unfurl("http://site.test/a").await.is_err());
        assert!(u.parse_link("http://site.test/a").is_none());
        live.store(sr_net::HostLists::default());
        assert!(u.unfurl("http://site.test/a").await.is_ok());
        // The rate budget can change too.
        u.set_per_host_per_minute(1);
        assert!(u.parse_link("http://site.test/b").is_some());
    }

    #[tokio::test]
    async fn failures_are_not_refetched() {
        let (u, t) = unfurler(vec![], 10);
        assert!(u.unfurl("http://site.test/missing").await.is_err());
        assert!(u.unfurl("http://site.test/missing").await.is_err());
        assert_eq!(t.seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn private_media_urls_are_not_embedded() {
        let page = html(
            r#"<head><title>T</title><meta property="og:image" content="http://127.0.0.1/x.png"></head>"#,
        );
        let (u, _) = unfurler(vec![("http://site.test/p", page)], 10);
        let e = u.unfurl("http://site.test/p").await.unwrap().unwrap();
        assert!(e.thumbnail.is_none() && e.image.is_none());
    }

    #[tokio::test]
    async fn nothing_to_show_is_none() {
        let (u, _) = unfurler(
            vec![("http://site.test/p", html("<head></head><body>hi</body>"))],
            10,
        );
        assert_eq!(u.unfurl("http://site.test/p").await.unwrap(), None);
    }

    #[tokio::test]
    async fn oembed_fills_a_thin_page() {
        let page = html(
            r#"<head><link rel="alternate" type="application/json+oembed" href="http://site.test/oe"></head>"#,
        );
        let oe = HopResponse {
            status: 200,
            content_type: Some("application/json".into()),
            body: br#"{"type":"rich","title":"From oEmbed","provider_name":"Prov","author_name":"Au"}"#.to_vec(),
            ..HopResponse::default()
        };
        let (u, _) = unfurler(
            vec![("http://site.test/p", page), ("http://site.test/oe", oe)],
            10,
        );
        let e = u.unfurl("http://site.test/p").await.unwrap().unwrap();
        assert_eq!(e.kind, "rich");
        assert_eq!(e.title.as_deref(), Some("From oEmbed"));
        assert_eq!(e.provider.name, "Prov");
        assert_eq!(e.author.unwrap().name, "Au");
    }

    fn media(ct: &str) -> HopResponse {
        HopResponse {
            status: 200,
            content_type: Some(ct.into()),
            ..HopResponse::default()
        }
    }

    #[tokio::test]
    async fn youtube_card_offers_play_here_and_drops_the_og_video_frame() {
        let page = html(
            r#"<head><meta property="og:title" content="A song">
            <meta property="og:type" content="video.other">
            <meta property="og:video" content="https://www.youtube.com/embed/dQw4w9WgXcQ">
            <meta property="og:image" content="https://img.test/t.jpg"></head>"#,
        );
        let (u, _) = unfurler(
            vec![("https://www.youtube.com/watch?v=dQw4w9WgXcQ", page)],
            10,
        );
        let e = u
            .unfurl("https://www.youtube.com/watch?v=dQw4w9WgXcQ")
            .await
            .unwrap()
            .unwrap();
        let play = e.play.clone().unwrap();
        assert_eq!(play.provider, "youtube");
        assert_eq!(
            play.src,
            "https://www.youtube-nocookie.com/embed/dQw4w9WgXcQ"
        );
        assert!(
            e.video.is_none(),
            "the page's own iframe address is not kept"
        );
        assert_eq!(e.title.as_deref(), Some("A song"));
        let j = serde_json::to_value(&e).unwrap();
        assert_eq!(j["sr_play"]["mode"], "embed");
        assert_eq!(j["sr_play"]["sync"], true);
    }

    #[tokio::test]
    async fn a_provider_link_still_gets_a_card_when_its_page_cannot_be_read() {
        let (u, _) = unfurler(vec![], 10);
        let e = u
            .unfurl("https://vimeo.com/76979871")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(e.provider.name, "Vimeo");
        assert_eq!(
            e.play.unwrap().src,
            "https://player.vimeo.com/video/76979871?dnt=1"
        );
        // A plain failing page without a provider is still an error.
        assert!(u.unfurl("https://media.test/gone").await.is_err());
    }

    #[tokio::test]
    async fn peertube_hosts_come_from_the_configuration() {
        let page = html(r#"<head><title>PT</title></head>"#);
        let (u, _) = unfurler(vec![("https://media.test/w/9c9de5e8-0a1e", page)], 10);
        let plain = u
            .unfurl("https://media.test/w/9c9de5e8-0a1e")
            .await
            .unwrap()
            .unwrap();
        assert!(plain.play.is_none());
        let (u, _) = unfurler(
            vec![(
                "https://media.test/w/9c9de5e8-0a1e",
                html(r#"<head><title>PT</title></head>"#),
            )],
            10,
        );
        let u = u.with_peertube_hosts(vec!["media.test".into()]);
        let e = u
            .unfurl("https://media.test/w/9c9de5e8-0a1e")
            .await
            .unwrap()
            .unwrap();
        let p = e.play.unwrap();
        assert_eq!(p.provider, "peertube");
        assert!(!p.sync);
    }

    #[tokio::test]
    async fn direct_media_is_offered_by_content_type_from_the_headers() {
        let (u, t) = unfurler(
            vec![
                ("https://media.test/clip", media("video/mp4")),
                ("https://media.test/song", media("audio/mpeg")),
                (
                    "https://media.test/live",
                    media("application/vnd.apple.mpegurl"),
                ),
                (
                    "https://media.test/page",
                    html("<head><title>x</title></head>"),
                ),
                ("http://media.test/plain.mp3", media("audio/mpeg")),
            ],
            20,
        );
        let v = u.unfurl("https://media.test/clip").await.unwrap().unwrap();
        assert_eq!(v.kind, "video");
        let p = v.play.unwrap();
        assert_eq!(
            (p.mode, p.media),
            (embed::PlayMode::Direct, Some(embed::DirectMedia::Video))
        );
        assert_eq!(p.src, "https://media.test/clip");
        let a = u.unfurl("https://media.test/song").await.unwrap().unwrap();
        assert_eq!(a.kind, "audio");
        assert_eq!(a.play.unwrap().media, Some(embed::DirectMedia::Audio));
        let h = u.unfurl("https://media.test/live").await.unwrap().unwrap();
        assert_eq!(h.kind, "video");
        assert_eq!(h.play.unwrap().media, Some(embed::DirectMedia::Hls));
        // No body is read for media: the transport saw one request each, and the page has no offer.
        assert_eq!(t.seen.lock().unwrap().len(), 3);
        let page = u.unfurl("https://media.test/page").await.unwrap().unwrap();
        assert!(page.play.is_none());
        // Plain http is previewed but not offered for playback.
        let plain = u
            .unfurl("http://media.test/plain.mp3")
            .await
            .unwrap()
            .unwrap();
        assert!(plain.play.is_none());
    }

    #[test]
    fn limiter_window() {
        let l = HostLimiter::new(2);
        let t0 = Instant::now();
        assert!(l.allow("a", t0));
        assert!(l.allow("A", t0));
        assert!(!l.allow("a", t0));
        assert!(l.allow("b", t0));
        assert!(l.allow("a", t0 + Duration::from_secs(61)));
    }
}

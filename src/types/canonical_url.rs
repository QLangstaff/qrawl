//! [`CanonicalUrl`]: a URL in the one form two URLs of the same page share,
//! parsed and normalized once.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use url::Url;

use super::{CanonicalDomain, SocialPlatform};
use crate::utils::{absolute_url, pinterest_pin_id, social_platform};

/// A URL in its canonical form: the key two URLs of the same page share. It
/// identifies a page; it isn't the address to fetch, since a server can
/// answer `http://`, `www.`, and a trailing slash differently.
///
/// The only way to make one is [`CanonicalUrl::new`], which refuses what
/// isn't an `http(s)` URL, so every `CanonicalUrl` has a host.
#[derive(Debug, Clone)]
pub struct CanonicalUrl {
    canonical: String,
    parsed: Url,
}

impl CanonicalUrl {
    /// `url` in canonical form, or `None` when it isn't an `http(s)` URL
    /// (`mailto:`, a relative path, a bare word).
    ///
    /// - `https`, whatever the scheme given; `https://` when none is given
    /// - the host lowercased, IDNA-encoded, without a root dot or `www.`
    /// - no trailing slash, no fragment
    /// - no tracking parameters; the rest sorted by key
    /// - on a social platform, no mobile host prefix and no share tokens
    pub fn new(url: &str) -> Option<CanonicalUrl> {
        let general = Self::general(url)?;
        let Some(platform) = social_platform(&general) else {
            return Some(general);
        };
        // Social rules apply only to recognized platforms, so a plain web
        // page keeps a functional `?s=` / `?ref=` and its `m.` host.
        let mut parsed = general.parsed;
        let host = parsed.host_str()?.to_string();
        // The `vm.`/`vt.` shorteners are kept on purpose.
        let mut desktop_host = host.as_str();
        while let Some(stripped) = desktop_host
            .strip_prefix("m.")
            .or_else(|| desktop_host.strip_prefix("mobile."))
            .or_else(|| desktop_host.strip_prefix("www."))
        {
            desktop_host = stripped;
        }
        parsed.set_host(Some(desktop_host)).ok()?;
        let query = kept_query(&parsed, |key| !is_social_tracker(key), false);
        parsed.set_query(query.as_deref());
        if platform == SocialPlatform::Pinterest {
            if let Some(path) = pinterest_pin_path(parsed.path()) {
                parsed.set_path(&path);
            }
        }
        Some(Self::from_parsed(parsed))
    }

    /// The canonical form every URL gets, before the social rules.
    fn general(url: &str) -> Option<CanonicalUrl> {
        let mut parsed = Url::parse(&absolute_url(url)?).ok()?;
        parsed.set_scheme("https").ok()?;
        // The host lowercased and IDNA-encoded, without the root dot of a fully
        // qualified name or a `www.` prefix (`WWW.Example.COM.` →
        // `example.com`). `www.com` keeps its `www.`: without it the host
        // would be a bare TLD.
        let lower = parsed
            .host_str()?
            .trim_end_matches('.')
            .to_ascii_lowercase();
        let encoded = idna::domain_to_ascii(&lower).unwrap_or(lower);
        let mut host = encoded.as_str();
        while let Some(stripped) = host.strip_prefix("www.").filter(|rest| rest.contains('.')) {
            host = stripped;
        }
        parsed.set_host(Some(host)).ok()?;
        let path = parsed.path().trim_end_matches('/').to_string();
        parsed.set_path(&path);
        // The pairs stay percent-encoded, so `?q=a%26b` keeps its one value.
        let query = kept_query(&parsed, |key| !is_tracking_param(key), true);
        parsed.set_query(query.as_deref());
        parsed.set_fragment(None);
        Some(Self::from_parsed(parsed))
    }

    fn from_parsed(parsed: Url) -> CanonicalUrl {
        CanonicalUrl {
            canonical: serialize(&parsed),
            parsed,
        }
    }

    /// The host: lowercased, IDNA-encoded, without a root dot or `www.`.
    pub fn host(&self) -> &str {
        self.parsed.host_str().unwrap_or_default()
    }

    /// Whether the host is `domain` or a subdomain of it (`old.reddit.com`
    /// has the domain `reddit.com`; `notreddit.com` doesn't).
    pub fn has_domain(&self, domain: &CanonicalDomain) -> bool {
        self.host() == domain.as_str()
            || self
                .host()
                .strip_suffix(domain.as_str())
                .is_some_and(|prefix| prefix.ends_with('.'))
    }

    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    pub fn into_inner(self) -> String {
        self.canonical
    }

    /// The parsed URL, for its path and query.
    pub(crate) fn as_url(&self) -> &Url {
        &self.parsed
    }
}

impl PartialEq for CanonicalUrl {
    fn eq(&self, other: &Self) -> bool {
        self.canonical == other.canonical
    }
}

impl Eq for CanonicalUrl {}

impl std::hash::Hash for CanonicalUrl {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.canonical.hash(state);
    }
}

impl std::fmt::Display for CanonicalUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.canonical)
    }
}

impl Serialize for CanonicalUrl {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.canonical)
    }
}

impl<'de> Deserialize<'de> for CanonicalUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let url = String::deserialize(deserializer)?;
        CanonicalUrl::new(&url)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid URL: {url}")))
    }
}

/// Tracking / analytics query parameters stripped so the
/// same page reached via different campaigns/referrers canonicalizes identically.
///
/// Coverage is the top ~10 sources that dominate real-world noise; not exhaustive.
/// In addition to this exact-match list, any param whose name starts with `utm_`
/// is stripped (covers the full Urchin Tracking Module family).
const TRACKING_PARAM_PREFIXES: &[&str] = &["utm_"];
const TRACKING_PARAM_NAMES: &[&str] = &[
    "fbclid",  // Facebook click ID
    "gclid",   // Google Ads click ID
    "mc_eid",  // Mailchimp email ID
    "mc_cid",  // Mailchimp campaign ID
    "_ga",     // Google Analytics
    "igshid",  // Instagram share ID
    "ref_src", // Twitter/X referrer source
    "ref_url", // generic referrer URL
];

fn is_tracking_param(key: &str) -> bool {
    TRACKING_PARAM_PREFIXES
        .iter()
        .any(|prefix| key.starts_with(prefix))
        || TRACKING_PARAM_NAMES.contains(&key)
}

/// `url` as a string, without the `/` the `url` crate writes for an empty
/// path. A query keeps it (`https://example.com/?a=1`), so a slash inside a
/// query value is never mistaken for it.
fn serialize(url: &Url) -> String {
    let serialized = url.to_string();
    match (url.path(), url.query()) {
        ("/", None) => serialized.trim_end_matches('/').to_string(),
        _ => serialized,
    }
}

/// The query of `url` with only the pairs whose key `keep` accepts, as written
/// (still percent-encoded), sorted by key when `sort`. `None` when no pair is
/// left.
fn kept_query(url: &Url, keep: impl Fn(&str) -> bool, sort: bool) -> Option<String> {
    let key_of = |pair: &str| pair.split('=').next().unwrap_or(pair).to_string();
    let mut pairs: Vec<&str> = url
        .query()?
        .split('&')
        .filter(|pair| !pair.is_empty() && keep(&key_of(pair)))
        .collect();
    if sort {
        pairs.sort_by_key(|pair| key_of(pair));
    }
    (!pairs.is_empty()).then(|| pairs.join("&"))
}

/// Social share / referrer tokens [`CanonicalUrl::new`] strips on top of the
/// general trackers, for recognized platforms only.
fn is_social_tracker(key: &str) -> bool {
    matches!(
        key,
        "si" | "igsh"
            | "s"
            | "_t"
            | "_r"
            | "ref"
            | "share_id"
            | "share_app_id"
            | "is_from_webapp"
            | "sender_device"
            | "feature"
            | "rdt"
            | "spm"
    )
}

/// A Pinterest pin's path with its pin segment reduced to the id
/// (`/pin/<title>--<id>` → `/pin/<id>`): a pin's title can change, its id
/// can't. `None` when `path` is no pin's.
fn pinterest_pin_path(path: &str) -> Option<String> {
    let (segment, rest) = path
        .strip_prefix("/pin/")
        .map(|after| after.split_once('/').unwrap_or((after, "")))?;
    let id = pinterest_pin_id(segment)?;
    Some(match rest {
        "" => format!("/pin/{id}"),
        rest => format!("/pin/{id}/{rest}"),
    })
}

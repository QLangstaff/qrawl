//! Shared Hosts

/// The lowercased host of `url`, without the root dot of a fully qualified
/// name (`example.com.`), or `None` when `url` doesn't parse or has no host
/// (`mailto:`, `file:`).
pub(crate) fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url)
        .ok()?
        .host_str()
        .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
}

/// Whether `host` is `domain` or a subdomain of it (`old.reddit.com` matches
/// `reddit.com`; `notreddit.com` doesn't).
pub(crate) fn host_matches(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|prefix| prefix.ends_with('.'))
}

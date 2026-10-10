use crate::tools::extract::{extract_open_graph, extract_social_parts};
use crate::types::{
    CanonicalUrl, ContentSource, Context, Html, PageContent, SocialKind, SocialParts,
    SocialPlatform,
};

use super::qrawl_any_url::qrawl_any_url;
use super::qrawl_many_urls::{qrawl_many_urls, schema_passes};
use super::qrawl_share::{qrawl_share, share_payload_only, with_caption};

/// Classify a raw URL into `(canonical, social)` — the pair the assembly
/// takes.
fn classify(url: &str) -> (CanonicalUrl, Option<SocialParts>) {
    let canonical = CanonicalUrl::new(url);
    let social = extract_social_parts(canonical.as_str());
    (canonical, social)
}

// ===== with_social mapping (pure) =====

#[test]
fn with_social_maps_facts_and_leaves_extras_empty() {
    let (canonical, social) = classify("https://reddit.com/r/recipes/comments/abc/best-soup");
    let page = PageContent {
        title: Some("Best Soup".to_string()),
        text: Some("1 onion, diced…".to_string()),
        author: Some("u/chef".to_string()),
        image_url: Some("https://img.example/x.jpg".to_string()),
        ..PageContent::new(canonical.clone(), ContentSource::RedditJson)
    }
    .with_social(social.as_ref());
    assert_eq!(page.platform, Some(SocialPlatform::Reddit));
    assert_eq!(page.source, ContentSource::RedditJson);
    assert_eq!(page.url, canonical);
    assert_eq!(page.title.as_deref(), Some("Best Soup"));
    assert_eq!(page.text.as_deref(), Some("1 onion, diced…"));
    assert_eq!(page.author.as_deref(), Some("u/chef"));
    // The page itself isn't scraped on a dedicated-door path.
    assert!(page.structured.is_empty());
    assert!(page.metadata.is_empty());
    assert!(page.body.is_none());
}

#[test]
fn with_social_author_falls_back_to_handle() {
    let (canonical, social) = classify("https://www.tiktok.com/@chef/video/123");
    let page = PageContent {
        title: Some("Quick pasta".to_string()),
        // extractor carried no author …
        ..PageContent::new(canonical, ContentSource::Oembed)
    }
    .with_social(social.as_ref());
    // … so it falls back to the classified handle.
    assert_eq!(page.author.as_deref(), Some("@chef"));
    assert_eq!(page.kind, Some(SocialKind::Video));
}

// ===== page-scrape assembly (pure) — extract_open_graph → with_social plus
// struct update for the extras, the assembly `extract_page_content` owns. =====

#[test]
fn page_scrape_assembly_carries_structured_and_optional_body() {
    // example.com is non-social → no social facts, so no platform/kind.
    let (canonical, social) = classify("https://example.com/recipe");
    assert!(social.is_none());
    let structured = vec![serde_json::json!({"@type": "Recipe", "name": "Soup"})];
    let metadata = vec![
        ("og:title".to_string(), "Soup".to_string()),
        (
            "og:image".to_string(),
            "https://img.example/x.jpg".to_string(),
        ),
    ];

    // Lean spine: structured carried, body skipped.
    let socials = vec!["https://instagram.com/chef".to_string()];
    let lean = PageContent {
        source: ContentSource::Html,
        structured: structured.clone(),
        metadata: metadata.clone(),
        socials: socials.clone(),
        ..extract_open_graph(&canonical, &metadata).with_social(social.as_ref())
    };
    assert_eq!(lean.source, ContentSource::Html);
    assert_eq!(lean.structured.len(), 1);
    assert_eq!(lean.socials, socials);
    assert!(lean.body.is_none());
    assert_eq!(lean.title.as_deref(), Some("Soup"));
    assert_eq!(lean.image_url.as_deref(), Some("https://img.example/x.jpg"));
    // No social facts: platform/kind are None.
    assert_eq!(lean.platform, None);
    assert_eq!(lean.kind, None);

    // Thorough spine: same fields plus body.
    let full = PageContent {
        body: Some(Html::new("body text".to_string())),
        ..extract_open_graph(&canonical, &metadata).with_social(social.as_ref())
    };
    assert_eq!(full.body.as_ref().map(|b| b.as_str()), Some("body text"));
}

#[test]
fn page_scrape_assembly_social_url_carries_platform() {
    // Social facts thread platform/kind into the page.
    let (canonical, social) = classify("https://www.tiktok.com/@chef/video/123");
    let page = extract_open_graph(&canonical, &Vec::new()).with_social(social.as_ref());
    assert_eq!(page.platform, Some(SocialPlatform::TikTok));
    assert_eq!(page.kind, Some(SocialKind::Video));
}

#[test]
fn page_content_serializes_platform_kind_per_arm() {
    // Wire-format guard. A social page emits the platform/kind strings; a
    // non-social page emits JSON `null` for both (previously the dropped
    // `"Other"`/`"Unknown"` sentinels). Host decoders must treat them as
    // optional. Dropping the sentinels changed this wire shape on purpose.
    let (canonical, social) = classify("https://www.tiktok.com/@u/video/1");
    let sjson = serde_json::to_value(
        extract_open_graph(&canonical, &Vec::new()).with_social(social.as_ref()),
    )
    .unwrap();
    assert_eq!(sjson["platform"], "TikTok");
    assert_eq!(sjson["kind"], "Video");

    let (wcanonical, wsocial) = classify("https://example.com/recipe");
    let wjson = serde_json::to_value(
        extract_open_graph(&wcanonical, &Vec::new()).with_social(wsocial.as_ref()),
    )
    .unwrap();
    assert!(
        wjson["platform"].is_null(),
        "web platform must serialize as null"
    );
    assert!(wjson["kind"].is_null(), "web kind must serialize as null");
}

// ===== schema_passes (pure) =====

#[test]
fn schema_passes_include_exclude() {
    use serde_json::json;
    let recipe = vec![json!({"@type": "Recipe", "name": "X"})];
    let article = vec![json!({"@type": "Article", "name": "Y"})];

    assert!(schema_passes(&recipe, &[], &[]), "no filters → pass");
    assert!(
        schema_passes(&recipe, &["Recipe".to_string()], &[]),
        "include match"
    );
    assert!(
        !schema_passes(&article, &["Recipe".to_string()], &[]),
        "include miss"
    );
    assert!(
        !schema_passes(&recipe, &[], &["Recipe".to_string()]),
        "exclude drops"
    );
    assert!(
        !schema_passes(&recipe, &["Recipe".to_string()], &["Recipe".to_string()]),
        "exclude beats include"
    );
}

// ===== Share layer (pure: caption + degrade fallback) =====

#[test]
fn share_payload_only_classifies_and_tags_source() {
    let p = share_payload_only(&CanonicalUrl::new("https://x.com/jack/status/20"));
    assert_eq!(p.source, ContentSource::SharePayloadOnly);
    assert_eq!(p.platform, Some(SocialPlatform::Twitter));
    // Author falls back to the classified handle.
    assert_eq!(p.author.as_deref(), Some("@jack"));
    // Nothing was fetched, so the page extras stay empty.
    assert!(p.structured.is_empty());
    assert!(p.body.is_none());
}

#[test]
fn share_payload_only_web_url_has_no_platform() {
    let p = share_payload_only(&CanonicalUrl::new("https://example.com/article"));
    assert_eq!(p.platform, None);
    assert_eq!(p.kind, None);
    assert!(p.author.is_none());
}

#[test]
fn with_caption_fills_missing_text() {
    let p = share_payload_only(&CanonicalUrl::new("https://example.com/x"));
    assert!(p.text.is_none());
    let p = with_caption(p, Some("First line\nsecond line".to_string()));
    // Whole caption as text; a caption has no title of its own.
    assert_eq!(p.text.as_deref(), Some("First line\nsecond line"));
    assert!(p.title.is_none());
}

#[test]
fn with_caption_does_not_override_page_text() {
    let mut p = share_payload_only(&CanonicalUrl::new("https://example.com/x"));
    p.text = Some("real page text".to_string());
    let p = with_caption(p, Some("a caption".to_string()));
    assert_eq!(p.text.as_deref(), Some("real page text"));
}

// ===== Live smoke tests (hit the network; run with `--ignored`) =====
//
// The pure layers (classify, side-door routing, mapping, share) are covered
// above; these exercise the end-to-end orchestration
// (scope → route → fetch/door → scrape → assemble).

#[tokio::test]
#[ignore = "hits the network"]
async fn live_share_reddit_subreddit() {
    let page = qrawl_share("https://www.reddit.com/r/rust", None, Context::fast())
        .await
        .expect("ok");
    assert_eq!(page.platform, Some(SocialPlatform::Reddit));
    // RedditJson from a residential IP; datacenter traffic 403s and self-heals
    // to the page scrape. Either way it must return content.
    assert!(page.title.is_some(), "expected content for an r/rust share");
}

#[tokio::test]
#[ignore = "hits the network"]
async fn live_share_youtube_oembed() {
    let page = qrawl_share(
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        None,
        Context::fast(),
    )
    .await
    .expect("ok");
    assert_eq!(page.platform, Some(SocialPlatform::YouTube));
    assert_eq!(page.source, ContentSource::Oembed);
    assert!(page.title.is_some(), "expected an oEmbed title");
}

#[tokio::test]
#[ignore = "hits the network"]
async fn live_blog_exposes_structured_recipe() {
    // One of the longest-lived recipe URLs on the web (numeric id is stable).
    let page = qrawl_any_url(
        "https://www.allrecipes.com/recipe/10813/best-chocolate-chip-cookies/",
        Context::auto(),
    )
    .await
    .expect("ok");
    assert_eq!(page.source, ContentSource::Html);
    assert!(
        !page.structured.is_empty(),
        "expected schema.org structured data on a recipe page"
    );
    assert!(page.body.is_some(), "expected scraped body text");
}

#[tokio::test]
#[ignore = "hits the network"]
async fn live_reddit_post_uses_json_door() {
    let page = qrawl_any_url("https://www.reddit.com/r/recipes", Context::fast())
        .await
        .expect("ok");
    assert_eq!(page.platform, Some(SocialPlatform::Reddit));
    // From a residential IP this is RedditJson; datacenter traffic 403s and
    // self-heals to the page scrape. Either way it must return a title.
    assert!(
        page.title.is_some(),
        "expected content for an r/recipes share"
    );
}

#[tokio::test]
#[ignore = "hits the network"]
async fn live_tiktok_uses_oembed_door() {
    let page = qrawl_any_url(
        "https://www.tiktok.com/@recipes/video/7500976227814100231",
        Context::auto(),
    )
    .await
    .expect("ok");
    assert_eq!(page.platform, Some(SocialPlatform::TikTok));
    // Guards the www-stripping regression: must reach oEmbed, not fall back.
    assert_eq!(page.source, ContentSource::Oembed);
    assert!(page.title.is_some() || page.author.is_some());
    // oEmbed yields no page HTML, so there's nothing structured here — this
    // is exactly why a TikTok video can't become a recipe without a caption.
    assert!(page.structured.is_empty());
}

#[tokio::test]
#[ignore = "hits the network"]
async fn live_many_urls_concurrent_best_effort() {
    let pages = qrawl_many_urls(
        vec![
            "https://www.allrecipes.com/recipe/10813/best-chocolate-chip-cookies/".to_string(),
            "https://www.allrecipes.com/recipe/23600/worlds-best-lasagna/".to_string(),
            // Unresolvable host: classifies fine, fetch fails → dropped.
            "https://nope-zzz-does-not-exist.example/".to_string(),
        ],
        Context::auto(), // direct — no child discovery, no cap
    )
    .await;
    // Best-effort: the two real recipes return, the dead URL is dropped.
    assert_eq!(pages.len(), 2, "expected the bad URL to be dropped");
    assert!(pages.iter().all(|p| p.source == ContentSource::Html));
    assert!(pages.iter().any(|p| !p.structured.is_empty()));
    // The lean spine carries no body text.
    assert!(pages.iter().all(|p| p.body.is_none()));
}

#[tokio::test]
#[ignore = "hits the network"]
async fn live_many_urls_limit_early_exits() {
    // Three resolvable URLs but `limit = 1` → the crawl stops at the first
    // completed page and cancels the rest.
    let pages = qrawl_many_urls(
        vec![
            "https://www.allrecipes.com/recipe/10813/best-chocolate-chip-cookies/".to_string(),
            "https://www.allrecipes.com/recipe/23600/worlds-best-lasagna/".to_string(),
            "https://www.allrecipes.com/recipe/220751/quick-chicken-piccata/".to_string(),
        ],
        Context::auto().with_limit(1),
    )
    .await;
    assert_eq!(pages.len(), 1, "limit should cap the results");
}

#[tokio::test]
#[ignore = "hits the network"]
async fn live_many_urls_depth_one_expands_and_respects_limit() {
    // Depth 1 expands a seed index page's children; `limit` caps the total.
    // We assert the bounded properties (runs end-to-end, honors the cap)
    // rather than an exact count, which depends on the live link graph.
    let pages = qrawl_many_urls(
        vec!["https://www.allrecipes.com/recipes/".to_string()],
        Context::auto().with_depth(1).with_limit(5),
    )
    .await;
    assert!(!pages.is_empty(), "depth-1 crawl should yield pages");
    assert!(pages.len() <= 5, "limit must cap the crawl");
    assert!(pages.iter().all(|p| p.source == ContentSource::Html));
}

#[tokio::test]
async fn limit_zero_returns_nothing() {
    let pages = qrawl_many_urls(
        vec!["https://example.invalid".to_string()],
        Context::fast().with_limit(0),
    )
    .await;
    assert!(pages.is_empty());
}

#[test]
fn parse_url_rejects_non_http_schemes() {
    use super::utils::parse_url;
    assert!(parse_url("https://example.com/a").is_ok());
    assert!(parse_url("example.com/a").is_ok());
    for input in ["mailto:a@b.com", "ftp://example.com/file.txt", "not a url"] {
        assert!(parse_url(input).is_err(), "{input}");
    }
}

// ===== Offline crawls against a local site =====

mod local_site {
    use std::collections::HashSet;
    use std::num::NonZeroUsize;
    use std::time::Duration;

    use futures_util::StreamExt;

    use crate::templates::{
        qrawl_any_url, qrawl_children, qrawl_discover_children, qrawl_fetch_stream, qrawl_many_urls,
    };
    use crate::test_site::{listing_page, Reply, TestSite};
    use crate::tools::fetch::fetch_strategy;
    use crate::types::Context;

    fn ctx(concurrency: usize) -> Context {
        Context::fast().with_concurrency(NonZeroUsize::new(concurrency).unwrap())
    }

    fn paths(pages: &[crate::types::PageContent], port: u16) -> HashSet<String> {
        let prefix = format!("https://127.0.0.1:{port}");
        pages
            .iter()
            .map(|page| page.url.as_str().trim_start_matches(&prefix).to_string())
            .collect()
    }

    /// `/p{n}` lists 12 children `/p{n}/c{m}`; every other path is a leaf.
    fn parents_with_children(path: &str) -> Reply {
        let links: Vec<String> = match path.strip_prefix("/p") {
            Some(parent) if !parent.contains('/') => (0..12)
                .map(|child| format!("/p{parent}/c{child}"))
                .collect(),
            _ => Vec::new(),
        };
        Reply::Html(listing_page(path, "", &links))
    }

    #[tokio::test]
    async fn children_of_many_parents_on_one_host_all_arrive() {
        let site = TestSite::serve(parents_with_children).await;
        let parents: Vec<String> = (0..24)
            .map(|parent| site.url(&format!("/p{parent}")))
            .collect();

        let children =
            tokio::time::timeout(Duration::from_secs(30), qrawl_children(parents, ctx(8)))
                .await
                .expect("the crawl finishes");

        assert_eq!(children.len(), 24 * 12);
        assert_eq!(site.hits(), 24 + 24 * 12);
    }

    #[tokio::test]
    async fn childless_parent_is_its_own_child_and_fetched_once() {
        let site = TestSite::serve(parents_with_children).await;

        let children = qrawl_children(vec![site.url("/leaf")], ctx(4)).await;

        assert_eq!(children.len(), 1);
        assert_eq!(children[0].0, site.url("/leaf"));
        assert_eq!(site.hits(), 1);
    }

    #[tokio::test]
    async fn discover_children_yields_each_child_once_without_fetching_it() {
        let site = TestSite::serve(parents_with_children).await;

        let children: Vec<String> =
            qrawl_discover_children(vec![site.url("/p1"), site.url("/p1/")], ctx(4))
                .collect()
                .await;

        assert_eq!(children.len(), 12);
        assert_eq!(site.hits(), 1);
    }

    #[tokio::test]
    async fn dropping_a_stream_stops_its_fetches() {
        let site = TestSite::serve(|path| match path {
            "/good" => Reply::Html(listing_page("good", "", &[])),
            _ => Reply::NotFound,
        })
        .await;
        let urls: Vec<String> = std::iter::once(site.url("/good"))
            .chain((0..300).map(|missing| site.url(&format!("/missing{missing}"))))
            .collect();

        let first = qrawl_fetch_stream(futures_util::stream::iter(urls), ctx(4))
            .take(1)
            .collect::<Vec<_>>()
            .await;
        tokio::time::sleep(Duration::from_millis(500)).await;

        assert_eq!(first.len(), 1);
        assert!(site.hits() < 30, "{} requests after the drop", site.hits());
    }

    /// Seeds `/a` and `/b`; `/a` → `/c` → `/x`, `/b` → `/x`, `/x` → `/y`.
    /// `/x` is one link from `/b`, so depth 2 reaches `/y` — unless `/x` is
    /// first seen through the slower `/a` → `/c` route.
    fn diamond(path: &str) -> Reply {
        let filler = |name: &str| {
            (0..3)
                .map(|n| format!("/{name}-filler{n}"))
                .collect::<Vec<_>>()
        };
        let links = match path {
            "/a" => [vec!["/c".to_string()], filler("a")].concat(),
            "/b" => [vec!["/x".to_string()], filler("b")].concat(),
            "/c" => [vec!["/x".to_string()], filler("c")].concat(),
            "/x" => [vec!["/y".to_string()], filler("x")].concat(),
            _ => Vec::new(),
        };
        Reply::Html(listing_page(path, "", &links))
    }

    #[tokio::test]
    async fn a_page_is_expanded_at_the_shallowest_level_that_reaches_it() {
        for seeds in [["/a", "/b"], ["/b", "/a"]] {
            let site = TestSite::serve(diamond).await;
            let seeds = seeds.iter().map(|seed| site.url(seed)).collect();

            let pages = qrawl_many_urls(seeds, ctx(1).with_depth(2)).await;

            let crawled = paths(&pages, site.port());
            assert!(crawled.contains("/x"), "{crawled:?}");
            assert!(crawled.contains("/y"), "{crawled:?}");
        }
    }

    #[tokio::test]
    async fn depth_limit_and_url_filters_bound_the_crawl() {
        let site = TestSite::serve(diamond).await;
        let seeds = || vec![site.url("/a"), site.url("/b")];

        let depth_zero = qrawl_many_urls(seeds(), ctx(4)).await;
        assert_eq!(
            paths(&depth_zero, site.port()),
            HashSet::from(["/a".to_string(), "/b".to_string()])
        );

        let limited = qrawl_many_urls(seeds(), ctx(4).with_depth(2).with_limit(3)).await;
        assert_eq!(limited.len(), 3);

        let blocked = qrawl_many_urls(
            seeds(),
            ctx(4).with_depth(2).with_block_urls(vec![site.url("/x")]),
        )
        .await;
        let crawled = paths(&blocked, site.port());
        assert!(crawled.contains("/c"), "{crawled:?}");
        assert!(
            !crawled.contains("/x") && !crawled.contains("/y"),
            "{crawled:?}"
        );

        let allowed = qrawl_many_urls(
            seeds(),
            ctx(4)
                .with_depth(2)
                .with_allow_urls(vec![site.url("/a"), site.url("/c")]),
        )
        .await;
        assert_eq!(
            paths(&allowed, site.port()),
            HashSet::from(["/a".to_string(), "/c".to_string()])
        );
    }

    #[tokio::test]
    async fn schema_filter_keeps_matching_pages_and_still_follows_the_rest() {
        let site = TestSite::serve(|path| {
            let recipe =
                r#"<script type="application/ld+json">{"@type":"Recipe","name":"Soup"}</script>"#;
            match path {
                "/roundup" => Reply::Html(listing_page(
                    path,
                    "",
                    &[
                        "/soup".to_string(),
                        "/about".to_string(),
                        "/contact".to_string(),
                    ],
                )),
                "/soup" => Reply::Html(listing_page(path, recipe, &[])),
                _ => Reply::Html(listing_page(path, "", &[])),
            }
        })
        .await;

        let recipes = qrawl_many_urls(
            vec![site.url("/roundup")],
            ctx(4)
                .with_depth(1)
                .with_include_schemas(vec!["Recipe".to_string()]),
        )
        .await;

        assert_eq!(
            paths(&recipes, site.port()),
            HashSet::from(["/soup".to_string()])
        );
    }

    #[tokio::test]
    async fn a_blocked_domain_is_never_requested() {
        let site = TestSite::serve(diamond).await;

        let allowed = qrawl_any_url(&site.url("/a"), ctx(4)).await;
        assert!(allowed.is_ok());
        let hits = site.hits();
        assert!(hits > 0);

        let blocked = qrawl_any_url(
            &site.url("/b"),
            ctx(4).with_block_domains(vec!["127.0.0.1".to_string()]),
        )
        .await;
        assert!(blocked.is_err());
        assert_eq!(site.hits(), hits);
    }

    #[tokio::test]
    async fn a_redirect_to_a_blocked_domain_is_rejected() {
        let site = TestSite::serve(diamond).await;
        let target = format!("http://localhost:{}/a", site.port());
        let redirecting = TestSite::serve(move |_path| Reply::Redirect(target.clone())).await;

        let followed = qrawl_any_url(&redirecting.url("/go"), ctx(4)).await;
        assert!(followed.is_ok(), "{followed:?}");

        let blocked = qrawl_any_url(
            &redirecting.url("/go"),
            ctx(4).with_block_domains(vec!["localhost".to_string()]),
        )
        .await;
        assert!(blocked.is_err());
    }

    #[tokio::test]
    async fn one_crawl_fetches_a_url_once_and_distinct_urls_separately() {
        let site = TestSite::serve(|path| Reply::Html(listing_page(path, "", &[]))).await;
        let (plain, slash) = (site.url("/echo?tag=b"), site.url("/echo/?tag=b"));

        let (first, again, other) = ctx(4)
            .scope(async {
                (
                    fetch_strategy(&plain).await.expect("fetches"),
                    fetch_strategy(&plain).await.expect("fetches"),
                    fetch_strategy(&slash).await.expect("fetches"),
                )
            })
            .await;

        assert_eq!(first, again);
        assert!(first.as_str().contains("<title>/echo?tag=b</title>"));
        assert!(other.as_str().contains("<title>/echo/?tag=b</title>"));
        assert_eq!(site.hits(), 2);
    }

    #[tokio::test]
    async fn an_http_url_is_fetched_as_given() {
        let site = TestSite::serve(|path| Reply::Html(listing_page(path, "", &[]))).await;

        let page = qrawl_any_url(&site.url("/page/"), ctx(4))
            .await
            .expect("fetches");

        assert_eq!(page.title.as_deref(), Some("/page/"));
        assert_eq!(
            page.url.as_str(),
            format!("https://127.0.0.1:{}/page", site.port())
        );
    }
}

use crate::types::{Context, SocialKind, SocialPlatform};

use super::qrawl_any_url::qrawl_any_url;
use super::qrawl_many_urls::{qrawl_many_urls, schema_passes};

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

// ===== Live smoke tests (hit the network; run with `--ignored`) =====

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
    assert_eq!(page.platform, None);
    assert!(!page.structured.is_empty(), "a schema.org Recipe");
    assert!(page.body.is_some());
}

#[tokio::test]
#[ignore = "hits the network"]
async fn live_youtube_video_has_embed_and_page_fields() {
    let page = qrawl_any_url(
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
        Context::fast(),
    )
    .await
    .expect("ok");
    assert_eq!(page.platform, Some(SocialPlatform::YouTube));
    assert_eq!(page.kind, Some(SocialKind::Video));
    // The embed names the creator; the page alone says "YouTube".
    assert_eq!(page.author.as_deref(), Some("Rick Astley"));
    assert!(page.metadata.iter().any(|(key, _)| key == "oembed:title"));
    assert!(page.body.is_some(), "the page was fetched too");
}

#[tokio::test]
#[ignore = "hits the network"]
async fn live_tiktok_video_takes_its_title_from_the_embed() {
    let page = qrawl_any_url(
        "https://www.tiktok.com/@recipes/video/7500976227814100231",
        Context::auto(),
    )
    .await
    .expect("ok");
    assert_eq!(page.platform, Some(SocialPlatform::TikTok));
    // TikTok's page is an app shell titled "TikTok - Make Your Day"; the
    // caption comes from oEmbed, which 400s without `www.`.
    let title = page.title.expect("a title");
    assert!(!title.starts_with("TikTok"), "{title}");
    assert!(page.author.is_some());
}

#[tokio::test]
#[ignore = "hits the network"]
async fn live_reddit_subreddit_returns_content() {
    let page = qrawl_any_url("https://www.reddit.com/r/recipes", Context::fast())
        .await
        .expect("ok");
    assert_eq!(page.platform, Some(SocialPlatform::Reddit));
    assert_eq!(page.kind, Some(SocialKind::Collection));
    assert!(page.title.is_some());
}

#[tokio::test]
#[ignore = "hits the network"]
async fn live_many_urls_drops_the_url_that_fails() {
    let pages = qrawl_many_urls(
        vec![
            "https://www.allrecipes.com/recipe/10813/best-chocolate-chip-cookies/".to_string(),
            "https://www.allrecipes.com/recipe/23600/worlds-best-lasagna/".to_string(),
            "https://nope-zzz-does-not-exist.example/".to_string(),
        ],
        Context::auto(),
    )
    .await;
    assert_eq!(pages.len(), 2);
    assert!(pages.iter().all(|page| !page.structured.is_empty()));
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
    let given = parse_url("http://www.Example.com/a/").expect("an HTTP URL");
    assert_eq!(given.address, "http://www.Example.com/a/");
    assert_eq!(given.url.as_str(), "https://example.com/a");
    let bare = parse_url("example.com/a").expect("a URL without a scheme");
    assert_eq!(bare.address, "https://example.com/a");
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

    fn paths(pages: &[crate::templates::PageContent], port: u16) -> HashSet<String> {
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
    async fn page_content_composes_every_layer_of_a_page() {
        let site = TestSite::serve(|path| {
            let head = r#"<meta property="og:title" content="Soup night">
                <meta name="description" content="A soup recipe">
                <meta name="author" content="Jane">
                <meta property="og:image" content="https://img.example/soup.jpg">
                <script type="application/ld+json">{"@type":"Recipe","name":"Soup"}</script>"#;
            let body_links = [
                "https://www.instagram.com/janecooks/".to_string(),
                "https://www.instagram.com/janecooks".to_string(),
                "https://twitter.com/share?url=x".to_string(),
                "https://example.com/elsewhere".to_string(),
            ];
            Reply::Html(listing_page(path, head, &body_links))
        })
        .await;

        let page = qrawl_any_url(&site.url("/soup"), ctx(4))
            .await
            .expect("fetches");

        assert_eq!(page.platform, None);
        assert_eq!(page.kind, None);
        assert_eq!(page.title.as_deref(), Some("/soup"));
        assert_eq!(page.text.as_deref(), Some("A soup recipe"));
        assert_eq!(page.author.as_deref(), Some("Jane"));
        assert_eq!(
            page.image_url.as_deref(),
            Some("https://img.example/soup.jpg")
        );
        assert_eq!(page.structured.len(), 1);
        assert_eq!(page.structured[0]["name"], "Soup");
        assert!(page.metadata.iter().any(|(key, _)| key == "og:title"));
        assert_eq!(page.socials, ["https://instagram.com/janecooks"]);
        assert!(page
            .body
            .is_some_and(|body| body.as_str().contains("<article>")));
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

#[tokio::test]
async fn emails_come_from_the_pages_two_links_from_the_seed() {
    use crate::test_site::{listing_page, Reply, TestSite};
    let site = TestSite::serve(|path| match path {
        "/" => Reply::Html(listing_page(
            path,
            "",
            &[
                "/team".to_string(),
                "/blog".to_string(),
                "/shop".to_string(),
            ],
        )),
        "/team" => Reply::Html(listing_page(path, "", &["/contact".to_string()])),
        "/contact" => Reply::Html(listing_page(
            "contact",
            "",
            &["mailto:Hello@Example.com".to_string()],
        )),
        _ => Reply::Html(listing_page(
            "seed@example.com is not two links away",
            "",
            &[],
        )),
    })
    .await;

    let emails = super::qrawl_emails(vec![site.url("/")], Context::fast()).await;

    assert_eq!(emails, ["hello@example.com"]);
}

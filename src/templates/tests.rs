use crate::errors::QrawlError;
use crate::types::{Context, SocialKind, SocialPlatform};

use super::qrawl_any_url::qrawl_any_url;
use super::qrawl_many_urls::{qrawl_many_urls, schema_passes};
use super::types::{CrawlOptions, MAX_DEPTH};
use super::utils::parse_url;

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

#[test]
fn parse_url_keeps_the_address_and_keys_on_the_canonical_form() {
    let given = parse_url("http://www.Example.com/a/").expect("an HTTP URL");
    assert_eq!(given.address, "http://www.Example.com/a/");
    assert_eq!(given.url.as_str(), "https://example.com/a");
    let bare = parse_url("example.com/a").expect("a URL without a scheme");
    assert_eq!(bare.address, "https://example.com/a");
    let port = parse_url("localhost:8080/a").expect("a host with a port");
    assert_eq!(port.address, "https://localhost:8080/a");
}

#[test]
fn parse_url_rejects_what_is_not_an_http_url() {
    for input in [
        "mailto:a@b.com",
        "ftp://example.com/file.txt",
        "not a url",
        "a@b.com",
    ] {
        assert_eq!(
            parse_url(input).unwrap_err(),
            QrawlError::InvalidUrl(input.to_string())
        );
    }
}

#[test]
fn crawl_depth_is_capped() {
    let options = CrawlOptions::default().with_depth(MAX_DEPTH + 5);
    assert_eq!(options.depth, MAX_DEPTH);
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
        CrawlOptions::default(),
        Context::auto(),
    )
    .await;
    assert_eq!(pages.len(), 2);
    assert!(pages.iter().all(|page| !page.structured.is_empty()));
}

// ===== Offline crawls against a local site =====

mod local_site {
    use std::collections::HashSet;
    use std::num::NonZeroUsize;
    use std::time::Duration;

    use futures_util::StreamExt;

    use crate::errors::QrawlError;
    use crate::templates::utils::{crawl, Target, Visited};
    use crate::templates::{qrawl_any_url, qrawl_many_urls, CrawlOptions, PageContent};
    use crate::test_site::{listing_page, Reply, Request, TestSite};
    use crate::tools::fetch::fetch_strategy;
    use crate::tools::normalize::normalize_html;
    use crate::tools::scrape::scrape_all;
    use crate::types::{CanonicalDomain, Context};

    fn context(concurrency: usize) -> Context {
        Context::fast().with_concurrency(NonZeroUsize::new(concurrency).unwrap())
    }

    fn depth(depth: usize) -> CrawlOptions {
        CrawlOptions::default().with_depth(depth)
    }

    fn paths(pages: &[PageContent], port: u16) -> HashSet<String> {
        let prefix = format!("https://127.0.0.1:{port}");
        pages
            .iter()
            .map(|page| page.url.as_str().trim_start_matches(&prefix).to_string())
            .collect()
    }

    fn set(paths: &[&str]) -> HashSet<String> {
        paths.iter().map(|path| path.to_string()).collect()
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

        let pages = tokio::time::timeout(
            Duration::from_secs(30),
            qrawl_many_urls(parents, depth(1), context(8)),
        )
        .await
        .expect("the crawl finishes");

        assert_eq!(pages.len(), 24 + 24 * 12);
        assert_eq!(site.hits(), 24 + 24 * 12);
    }

    #[tokio::test]
    async fn reaching_the_limit_stops_the_fetches() {
        let site = TestSite::serve(parents_with_children).await;
        let urls: Vec<String> = (0..300)
            .map(|page| site.url(&format!("/leaf{page}")))
            .collect();

        let pages = qrawl_many_urls(urls, CrawlOptions::default().with_limit(1), context(4)).await;
        tokio::time::sleep(Duration::from_millis(500)).await;

        assert_eq!(pages.len(), 1);
        assert!(site.hits() < 30, "{} requests after the limit", site.hits());
    }

    async fn fetch_address(target: Target, _level: usize) -> Option<Visited<String>> {
        let fetched = fetch_strategy(&target.address).await.ok()?;
        Some(Visited {
            url: target.url,
            item: fetched.address,
            children: Vec::new(),
        })
    }

    #[tokio::test]
    async fn a_slow_consumer_still_gets_every_page() {
        let site = TestSite::serve(parents_with_children).await;
        let urls: Vec<String> = (0..12)
            .map(|page| site.url(&format!("/leaf{page}")))
            .collect();

        let mut pages = Box::pin(crawl(context(2), urls, 0, |_url| true, fetch_address));
        let mut received = 0;
        while pages.next().await.is_some() {
            received += 1;
            if received == 1 {
                // Longer than `Context::fast()`'s per-attempt timeout.
                tokio::time::sleep(Duration::from_secs(9)).await;
            }
        }

        assert_eq!(received, 12);
    }

    #[tokio::test]
    async fn a_page_that_panics_is_dropped_and_the_rest_arrive() {
        let site = TestSite::serve(parents_with_children).await;
        let urls: Vec<String> = (0..20)
            .map(|page| site.url(&format!("/leaf{page}")))
            .collect();

        let pages: Vec<(usize, String)> = crawl(
            context(4),
            urls,
            0,
            |_url| true,
            |target, level| async move {
                assert!(!target.address.ends_with("/leaf7"), "leaf7 panics");
                fetch_address(target, level).await
            },
        )
        .collect()
        .await;

        assert_eq!(pages.len(), 19);
    }

    /// Seeds `/a` and `/b`; `/a` → `/c` → `/x`, `/b` → `/x`, `/x` → `/y`.
    /// `/x` is one link from `/b`, so depth 2 reaches `/y` — unless `/x` is
    /// first seen through the slower `/a` → `/c` route.
    fn diamond(path: &str) -> Reply {
        let filler = |name: &str| {
            (0..3)
                .map(|number| format!("/{name}-filler{number}"))
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

            let pages = qrawl_many_urls(seeds, depth(2), context(1)).await;

            let crawled = paths(&pages, site.port());
            assert!(crawled.contains("/x"), "{crawled:?}");
            assert!(crawled.contains("/y"), "{crawled:?}");
        }
    }

    #[tokio::test]
    async fn depth_limit_and_url_filters_bound_the_crawl() {
        let site = TestSite::serve(diamond).await;
        let seeds = || vec![site.url("/a"), site.url("/b")];

        let depth_zero = qrawl_many_urls(seeds(), CrawlOptions::default(), context(4)).await;
        assert_eq!(paths(&depth_zero, site.port()), set(&["/a", "/b"]));

        let limited = qrawl_many_urls(seeds(), depth(2).with_limit(3), context(4)).await;
        assert_eq!(limited.len(), 3);

        let none = qrawl_many_urls(seeds(), depth(2).with_limit(0), context(4)).await;
        assert!(none.is_empty());

        let blocked = qrawl_many_urls(
            seeds(),
            depth(2).with_block_urls(vec![site.url("/x")]),
            context(4),
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
            depth(2).with_allow_urls(vec![site.url("/a"), site.url("/c")]),
            context(4),
        )
        .await;
        assert_eq!(paths(&allowed, site.port()), set(&["/a", "/c"]));
    }

    fn recipes_and_articles(path: &str) -> Reply {
        let schema = |schema_type: &str| {
            format!(
                r#"<script type="application/ld+json">{{"@type":"{schema_type}","name":"x"}}</script>"#
            )
        };
        match path {
            "/roundup" => Reply::Html(listing_page(
                path,
                "",
                &[
                    "/soup".to_string(),
                    "/stew".to_string(),
                    "/news".to_string(),
                    "/about".to_string(),
                ],
            )),
            "/soup" | "/stew" => Reply::Html(listing_page(path, &schema("Recipe"), &[])),
            "/news" => Reply::Html(listing_page(path, &schema("Article"), &[])),
            _ => Reply::Html(listing_page(path, "", &[])),
        }
    }

    #[tokio::test]
    async fn schema_filters_decide_results_and_the_limit_counts_only_results() {
        let site = TestSite::serve(recipes_and_articles).await;
        let seeds = || vec![site.url("/roundup")];

        let recipes = qrawl_many_urls(
            seeds(),
            depth(1).with_include_schemas(vec!["recipe".to_string()]),
            context(4),
        )
        .await;
        assert_eq!(paths(&recipes, site.port()), set(&["/soup", "/stew"]));

        let not_articles = qrawl_many_urls(
            seeds(),
            depth(1).with_exclude_schemas(vec!["Article".to_string()]),
            context(4),
        )
        .await;
        assert_eq!(
            paths(&not_articles, site.port()),
            set(&["/roundup", "/soup", "/stew", "/about"])
        );

        let one_recipe = qrawl_many_urls(
            seeds(),
            depth(1)
                .with_include_schemas(vec!["Recipe".to_string()])
                .with_limit(1),
            context(1),
        )
        .await;
        assert_eq!(one_recipe.len(), 1);
        assert!(set(&["/soup", "/stew"]).is_superset(&paths(&one_recipe, site.port())));
    }

    #[tokio::test]
    async fn a_blocked_domain_is_never_requested() {
        let site = TestSite::serve(diamond).await;

        let allowed = qrawl_any_url(&site.url("/a"), context(4)).await;
        assert!(allowed.is_ok());
        let hits = site.hits();
        assert!(hits > 0);

        for listed in ["127.0.0.1"] {
            let blocked = qrawl_any_url(
                &site.url("/b"),
                context(4).with_block_domains(vec![CanonicalDomain::new(listed).unwrap()]),
            )
            .await;
            assert!(
                matches!(blocked, Err(QrawlError::BlockedDomain(_))),
                "{blocked:?}"
            );
        }
        let allow_other = qrawl_any_url(
            &site.url("/b"),
            context(4).with_allow_domains(vec![CanonicalDomain::new("example.com").unwrap()]),
        )
        .await;
        assert!(matches!(allow_other, Err(QrawlError::BlockedDomain(_))));
        assert_eq!(site.hits(), hits);
    }

    #[tokio::test]
    async fn a_redirect_to_a_blocked_domain_is_never_requested() {
        let site = TestSite::serve(diamond).await;
        let target = format!("http://localhost:{}/a", site.port());
        let redirecting = TestSite::serve(move |_path| Reply::Redirect(target.clone())).await;

        let followed = qrawl_any_url(&redirecting.url("/go"), context(4)).await;
        assert!(followed.is_ok(), "{followed:?}");
        let hits = site.hits();
        assert!(hits > 0);

        let blocked = qrawl_any_url(
            &redirecting.url("/go"),
            context(4).with_block_domains(vec![CanonicalDomain::new("localhost").unwrap()]),
        )
        .await;
        assert!(
            matches!(blocked, Err(QrawlError::BlockedDomain(_))),
            "{blocked:?}"
        );
        assert_eq!(site.hits(), hits, "the blocked host got a request");
    }

    #[tokio::test]
    async fn relative_links_resolve_against_the_url_a_redirect_landed_on() {
        let site = TestSite::serve(|path| match path {
            "/blog" => Reply::Redirect("/blog/".to_string()),
            "/blog/" => Reply::Html(listing_page(
                path,
                "",
                &[
                    "post0".to_string(),
                    "post1".to_string(),
                    "post2".to_string(),
                ],
            )),
            _ => Reply::Html(listing_page(path, "", &[])),
        })
        .await;

        let pages = qrawl_many_urls(vec![site.url("/blog")], depth(1), context(4)).await;

        assert_eq!(
            paths(&pages, site.port()),
            set(&["/blog", "/blog/post0", "/blog/post1", "/blog/post2"])
        );
        let blog = pages
            .iter()
            .find(|page| page.title.as_deref() == Some("/blog/"));
        assert_eq!(
            blog.map(|page| page.address.clone()),
            Some(site.url("/blog/"))
        );
    }

    #[tokio::test]
    async fn a_missing_page_is_requested_once_under_every_strategy() {
        let site = TestSite::serve(|_path| Reply::NotFound).await;

        let missing = Context::auto()
            .scope(fetch_strategy(&site.url("/gone")))
            .await;

        assert_eq!(missing.unwrap_err(), QrawlError::Status(404));
        assert_eq!(site.hits(), 1);
    }

    #[tokio::test]
    async fn page_content_composes_every_layer_of_a_page() {
        let site = TestSite::serve(|path| {
            let head = r#"<meta property="og:title" content="Soup night">
                <meta name="description" content="A soup recipe">
                <meta name="author" content="Jane">
                <meta name="oembed:title" content="Not an embed">
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

        let page = qrawl_any_url(&site.url("/soup/"), context(4))
            .await
            .expect("fetches");

        assert_eq!(
            page.url.as_str(),
            format!("https://127.0.0.1:{}/soup", site.port())
        );
        assert_eq!(page.address, site.url("/soup/"));
        assert_eq!(page.platform, None);
        assert_eq!(page.kind, None);
        assert_eq!(page.title.as_deref(), Some("/soup/"));
        assert_eq!(page.text.as_deref(), Some("A soup recipe"));
        assert_eq!(page.author.as_deref(), Some("Jane"));
        assert_eq!(
            page.image_url.as_deref(),
            Some("https://img.example/soup.jpg")
        );
        assert_eq!(page.structured.len(), 1);
        assert_eq!(page.structured[0]["name"], "Soup");
        assert!(page.metadata.iter().any(|(key, _)| key == "og:title"));
        assert!(!page
            .metadata
            .iter()
            .any(|(key, _)| key.starts_with("oembed:")));
        assert_eq!(page.socials, ["https://instagram.com/janecooks"]);
        assert!(page
            .body
            .is_some_and(|body| body.as_str().contains("<article>")));
    }

    #[tokio::test]
    async fn urls_that_redirect_to_one_page_return_it_once() {
        let site = TestSite::serve(|path| match path {
            "/old" | "/old2" => Reply::Redirect("/new".to_string()),
            _ => Reply::Html(listing_page(path, "", &[])),
        })
        .await;
        let seeds = || vec![site.url("/old"), site.url("/new"), site.url("/old2")];

        let pages = qrawl_many_urls(seeds(), CrawlOptions::default(), context(1)).await;
        assert_eq!(paths(&pages, site.port()), set(&["/new"]));
        assert_eq!(pages.len(), 1);

        let moved = qrawl_any_url(&site.url("/old"), context(1))
            .await
            .expect("fetches");
        assert_eq!(
            moved.url.as_str(),
            format!("https://127.0.0.1:{}/new", site.port())
        );

        let blocked = qrawl_many_urls(
            vec![site.url("/old")],
            CrawlOptions::default().with_block_urls(vec![site.url("/new")]),
            context(1),
        )
        .await;
        assert!(blocked.is_empty());
    }

    #[tokio::test]
    async fn a_page_nested_too_deep_is_read_as_empty_not_a_crash() {
        let deep = format!(
            "<!DOCTYPE html><html><head><title>deep</title></head><body><div class=\"h-card\" itemscope>{}{}{}</div></body></html>",
            "<div>".repeat(2_000),
            "Lorem ipsum dolor sit amet. ".repeat(30),
            "</div>".repeat(2_000),
        );
        let site = TestSite::serve(move |_path| Reply::Html(deep.clone())).await;

        let pages = qrawl_many_urls(vec![site.url("/deep")], depth(1), context(1)).await;

        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].title, None);
        assert!(pages[0].structured.is_empty());
        let markdown = crate::tools::transform::transform_markdown(
            &fetch_strategy(&site.url("/deep"))
                .await
                .expect("fetches")
                .html,
        )
        .await;
        assert!(markdown.as_str().is_empty());
    }

    #[tokio::test]
    async fn chain_runs_every_step() {
        use crate::tools::extract::extract_emails;
        use crate::tools::map::map_children;
        use crate::tools::normalize::{dedupe_urls, normalize_emails};
        use crate::types::Html;
        let site = TestSite::serve(|path| match path {
            "/" => Reply::Html(listing_page(
                path,
                "",
                &[
                    "/team".to_string(),
                    "/team/".to_string(),
                    "/blog".to_string(),
                    "/shop".to_string(),
                ],
            )),
            "/team" | "/team/" => Reply::Html(listing_page(
                path,
                "",
                &["mailto:Hello@Example.com".to_string()],
            )),
            _ => Reply::Html(listing_page(path, "", &[])),
        })
        .await;
        let hits_before = site.hits();

        let emails = crate::chain! {
            vec![site.url("/")], context(4) =>
            all(dedupe_urls) -> fetch -> map(map_children) ->
            all(dedupe_urls) -> fetch ->
            filter(|html: Html| async move { html.as_str().contains("mailto:") }) ->
            extract(extract_emails) -> all(normalize_emails)
        };

        assert_eq!(emails, ["hello@example.com"]);
        // The seed, then `/team` once (its `/team/` twin is deduped), `/blog`, `/shop`.
        assert_eq!(site.hits() - hits_before, 4);
    }

    #[tokio::test]
    async fn a_redirect_yields_the_same_pages_whichever_fetch_finishes_first() {
        // `/p` lists `/c`; `/s` redirects to `/c`; `/c` lists `/d`.
        fn site_reply(path: &str) -> Reply {
            let filler = |name: &str| {
                (0..3)
                    .map(|number| format!("/{name}-filler{number}"))
                    .collect::<Vec<_>>()
            };
            match path {
                "/s" => Reply::Redirect("/c".to_string()),
                "/p" => Reply::Html(listing_page(
                    path,
                    "",
                    &[vec!["/c".to_string()], filler("p")].concat(),
                )),
                "/c" => Reply::Html(listing_page(
                    path,
                    "",
                    &[vec!["/d".to_string()], filler("c")].concat(),
                )),
                _ => Reply::Html(listing_page(path, "", &[])),
            }
        }
        let mut results = Vec::new();
        for seeds in [["/p", "/s"], ["/s", "/p"]] {
            let site = TestSite::serve(site_reply).await;
            let seeds = seeds.iter().map(|seed| site.url(seed)).collect();
            let pages = qrawl_many_urls(seeds, depth(1), context(1)).await;
            let crawled = paths(&pages, site.port());
            assert_eq!(pages.len(), crawled.len(), "a page was returned twice");
            results.push(crawled);
        }
        assert_eq!(results[0], results[1]);
        assert!(
            results[0].contains("/c") && results[0].contains("/d"),
            "{:?}",
            results[0]
        );
    }

    #[tokio::test]
    async fn a_redirect_onto_a_seed_that_fails_still_yields_the_page() {
        let site = TestSite::serve(|path| match path {
            "/old" => Reply::Redirect("/new".to_string()),
            "/new" => Reply::Html(listing_page(path, "", &[])),
            _ => Reply::NotFound,
        })
        .await;

        let pages = qrawl_many_urls(
            vec![site.url("/old"), site.url("/new/")],
            CrawlOptions::default(),
            context(1),
        )
        .await;

        assert_eq!(paths(&pages, site.port()), set(&["/new"]));
    }

    #[tokio::test]
    async fn a_blocked_redirect_is_blocked_under_the_auto_strategy_too() {
        let site = TestSite::serve(diamond).await;
        let target = format!("http://localhost:{}/a", site.port());
        let redirecting = TestSite::serve(move |_path| Reply::Redirect(target.clone())).await;

        let blocked = qrawl_any_url(
            &redirecting.url("/go"),
            Context::auto().with_block_domains(vec![CanonicalDomain::new("localhost").unwrap()]),
        )
        .await;

        assert!(
            matches!(blocked, Err(QrawlError::BlockedDomain(_))),
            "{blocked:?}"
        );
        assert_eq!(site.hits(), 0);
        assert_eq!(redirecting.hits(), 1);
    }

    #[tokio::test]
    async fn every_walker_survives_a_document_at_the_depth_bound() {
        use crate::tools::map::map_children;
        use crate::tools::scrape::scrape_all;
        use crate::tools::transform::transform_markdown;
        use crate::types::Html;
        let nested = |open: &str, close: &str, levels: usize| {
            Html::new(format!(
                "<html><body>{}text{}</body></html>",
                open.repeat(levels),
                close.repeat(levels)
            ))
        };
        // The deepest documents the bound admits.
        let microformats = nested("<div class=\"h-card p-a\">", "</div>", 500);
        let microdata = nested("<div itemscope itemprop=\"a\">", "</div>", 500);
        let tables = nested("<table><tr><td>", "</td></tr></table>", 120);

        for html in [&microformats, &microdata, &tables] {
            scrape_all(html).await;
            map_children(html, "https://example.com/").await;
            transform_markdown(html).await;
        }
    }

    #[tokio::test]
    async fn a_body_over_the_size_cap_is_refused_once() {
        let site = TestSite::serve(|path| match path {
            "/huge" => Reply::Html(format!(
                "<html><body>{}</body></html>",
                "x".repeat(crate::tools::fetch::MAX_RESPONSE_BYTES)
            )),
            _ => Reply::Html(listing_page(path, "", &[])),
        })
        .await;

        let huge = Context::auto()
            .scope(fetch_strategy(&site.url("/huge")))
            .await;
        assert!(matches!(huge, Err(QrawlError::TooLarge(_))), "{huge:?}");
        // Final: the cascade didn't try its other two profiles.
        assert_eq!(site.hits(), 1);
    }

    #[tokio::test]
    async fn a_redirect_loop_ends_with_the_first_profile_that_keeps_cookies() {
        let site = TestSite::serve(|_path| Reply::Redirect("/loop".to_string())).await;

        let looped = Context::auto()
            .scope(fetch_strategy(&site.url("/loop")))
            .await;
        assert!(
            matches!(looped, Err(QrawlError::TooManyRedirects(_))),
            "{looped:?}"
        );
        // Minimal follows 5, Windows 10; a third profile would make it 20 or more.
        assert!(
            (6..=16).contains(&site.hits()),
            "{} requests for one loop",
            site.hits()
        );
    }

    #[tokio::test]
    async fn a_redirect_that_waits_for_its_cookie_is_followed_by_a_profile_that_keeps_it() {
        let site = TestSite::serve_requests(|request: &Request| match request.cookie {
            Some(cookie) if cookie.contains("consent=yes") => {
                Reply::Html(listing_page(request.path, "", &[]))
            }
            _ => Reply::RedirectWithCookie(request.path.to_string(), "consent=yes"),
        })
        .await;

        let page = Context::auto()
            .scope(fetch_strategy(&site.url("/wall")))
            .await;
        assert!(page.is_ok(), "{page:?}");
    }

    /// A site whose `/story` redirects a visitor to its consent interstitial,
    /// and whose `/s` is a shortlink to `/item`.
    fn consent_wall(path: &str) -> Reply {
        match path {
            "/story" => Reply::Redirect("/consent/tcf/story".to_string()),
            "/s" => Reply::Redirect("/item".to_string()),
            "/consent/tcf/story" | "/item" => Reply::Html(listing_page(path, "", &[])),
            _ => Reply::NotFound,
        }
    }

    #[tokio::test]
    async fn a_redirect_onto_a_consent_interstitial_is_a_failed_fetch() {
        let site = TestSite::serve(consent_wall).await;

        let wall = Context::auto()
            .scope(fetch_strategy(&site.url("/story")))
            .await;
        assert_eq!(
            wall,
            Err(QrawlError::ConsentInterstitial(
                site.url("/consent/tcf/story")
            ))
        );
        // Two requests per profile. The first profile that keeps cookies ends
        // it; trying all three would make it 6.
        assert!(
            (2..=4).contains(&site.hits()),
            "{} requests for one wall",
            site.hits()
        );

        let fast = Context::fast()
            .scope(fetch_strategy(&site.url("/story")))
            .await;
        assert!(
            matches!(fast, Err(QrawlError::ConsentInterstitial(_))),
            "{fast:?}"
        );

        // An ordinary redirect is followed, and so is the interstitial's own
        // address when it is the one asked for.
        for (asked, served) in [
            ("/s", "/item"),
            ("/consent/tcf/story", "/consent/tcf/story"),
        ] {
            let page = Context::auto()
                .scope(fetch_strategy(&site.url(asked)))
                .await
                .unwrap_or_else(|error| panic!("{asked}: {error}"));
            assert_eq!(page.address, site.url(served));
        }
    }

    #[tokio::test]
    async fn a_consent_interstitial_is_not_stood_in_for_the_page() {
        let site = TestSite::serve(consent_wall).await;
        let page = qrawl_any_url(&site.url("/story"), Context::auto()).await;
        assert!(
            matches!(page, Err(QrawlError::ConsentInterstitial(_))),
            "{page:?}"
        );
    }

    #[tokio::test]
    async fn a_consent_interstitial_a_cookie_gets_past_is_followed_by_a_profile_that_keeps_it() {
        let site = TestSite::serve_requests(|request: &Request| {
            let consented = request
                .cookie
                .is_some_and(|cookie| cookie.contains("consent=yes"));
            match (request.path, consented) {
                ("/story", false) => {
                    Reply::RedirectWithCookie("/consent/tcf/story".to_string(), "consent=yes")
                }
                ("/consent/tcf/story", true) => Reply::Redirect("/story".to_string()),
                _ => Reply::Html(listing_page(request.path, "", &[])),
            }
        })
        .await;

        let page = Context::auto()
            .scope(fetch_strategy(&site.url("/story")))
            .await
            .expect("the profile that keeps the cookie reads the page");
        assert_eq!(page.address, site.url("/story"));
    }

    #[test]
    fn a_page_that_makes_the_parser_untangle_formatting_over_and_over_is_refused_early() {
        use crate::utils::parse_document_bounded;
        // The parser's cost grows with the square of this: at 30,000 it took
        // eight seconds to parse in full.
        let storm = format!(
            "<html><body>{}</body></html>",
            "<b><i><div></b>".repeat(30_000)
        );
        let started = std::time::Instant::now();
        assert!(parse_document_bounded(&storm).is_none());
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_page_that_makes_the_parser_rebuild_dozens_of_formatting_elements_per_paragraph_is_refused()
    {
        use crate::utils::parse_document_bounded;
        // Fourteen formatting tags left open three times each, then short
        // paragraphs: each rebuilds dozens of them, twenty operations a byte,
        // while the tree stays shallow.
        let formatting = "<a><b><big><code><em><font><i><nobr><s><small><strike><strong><tt><u>";
        let storm = format!(
            "<html><body><p>{}{}</body></html>",
            formatting.repeat(3),
            "<p>x".repeat(200_000)
        );
        let started = std::time::Instant::now();
        assert!(parse_document_bounded(&storm).is_none());
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn a_page_nested_far_too_deep_is_refused_before_it_is_parsed() {
        // 50,000 levels take the parser seconds; the tokens show the depth at once.
        let deep = format!(
            "<!DOCTYPE html><html><head><title>deep</title></head><body>{}x{}</body></html>",
            "<div>".repeat(50_000),
            "</div>".repeat(50_000)
        );
        let started = std::time::Instant::now();
        let (body, _, structured) = scrape_all(&deep.as_str().into()).await;
        assert!(!body.as_str().contains("<div>"), "{}", body.as_str().len());
        assert!(structured.is_empty());
        assert_eq!(normalize_html(&deep.as_str().into()).await.as_str(), "");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn markup_the_parser_flattens_is_not_too_deep_and_what_it_nests_is() {
        use crate::utils::parse_document_bounded;
        let is_too_deep_markup = |html: &str| parse_document_bounded(html).is_none();
        // Each of these the parser closes by implication: a flat document.
        for flat in [
            "<p>".repeat(5_000),
            format!("<ul>{}</ul>", "<li><span>x</li>".repeat(600)),
            "<p><b>x".repeat(600),
            format!("<table><tr>{}</tr></table>", "<td><span>x</td>".repeat(600)),
            format!("<table>{}</table>", "<tr><td><em>x".repeat(600)),
            "<a href=x>".repeat(600),
            format!("<select>{}</select>", "<option><span>x".repeat(600)),
            "<p>x<div>y</div>".repeat(600),
            format!("<svg>{}</svg>", "<path d=\"M0\"/>".repeat(1_000)),
            "<br>".repeat(1_000),
            // Inside a script, a style, or a textarea, tags are text.
            format!("<script>var s = '{}';</script>", "<div>".repeat(600)),
            format!("<style>{}</style>", "<div>".repeat(600)),
            format!("<textarea>{}</textarea>", "<div>".repeat(600)),
            format!("<title>{}</title>", "<div>".repeat(600)),
            format!("<noscript>{}</noscript>", "<div>".repeat(600)),
            // A heading ends the heading before it.
            "<h2>x".repeat(600),
            "<h1>a<h2>b<h3>c".repeat(200),
            format!("<button>{}</button>", "<p>x".repeat(600)),
            format!("<select>{}</select>", "<option><optgroup>".repeat(300)),
        ] {
            assert!(!is_too_deep_markup(&flat), "{}", &flat[..40]);
        }
        // Each of these the parser nests as written.
        for deep in [
            "<div>".repeat(600),
            "<div/>".repeat(600),
            "<ul><li>".repeat(600),
            "<dl><dt><span>x<dd><span>y".repeat(300),
            "<b>".repeat(600),
            "<table><tr><td>".repeat(300),
            "<p>x<div>y".repeat(600),
            "<li><ul>".repeat(600),
            format!("<template>{}</template>", "<div>".repeat(600)),
            format!("<svg>{}</svg>", "<g>".repeat(600)),
            "<span>x".repeat(600),
            // The adoption agency moves these deeper after placing them.
            "<b><i><div></b>".repeat(300),
            "<a><b><div></a>".repeat(300),
            // Exactly past the bound, counting the root and a text node.
            format!("{}x", "<div>".repeat(510)),
        ] {
            assert!(is_too_deep_markup(&deep), "{}", &deep[..40]);
        }
    }

    #[tokio::test]
    async fn the_limit_stops_a_crawl_of_urls_that_all_redirect() {
        let site = TestSite::serve(|path| match path.strip_prefix("/short") {
            Some(number) => Reply::Redirect(format!("/item{number}")),
            None => Reply::Html(listing_page(path, "", &[])),
        })
        .await;
        let urls: Vec<String> = (0..200)
            .map(|number| site.url(&format!("/short{number}")))
            .collect();

        let pages = qrawl_many_urls(urls, CrawlOptions::default().with_limit(1), context(2)).await;
        tokio::time::sleep(Duration::from_millis(500)).await;

        assert_eq!(pages.len(), 1);
        assert!(site.hits() < 30, "{} requests after the limit", site.hits());
    }

    #[tokio::test]
    async fn a_seed_a_redirect_already_landed_on_is_not_fetched_again() {
        let site = TestSite::serve(|path| match path {
            "/a" => Reply::Redirect("/b".to_string()),
            "/b" => Reply::Redirect("/c".to_string()),
            _ => Reply::Html(listing_page(path, "", &[])),
        })
        .await;

        let pages = qrawl_many_urls(
            vec![site.url("/a"), site.url("/c")],
            CrawlOptions::default(),
            context(1),
        )
        .await;

        assert_eq!(paths(&pages, site.port()), set(&["/c"]));
        // `/a`, `/b`, `/c` once each: the chain reached `/c` before its own visit.
        assert_eq!(site.hits(), 3);
    }
}

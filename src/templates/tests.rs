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

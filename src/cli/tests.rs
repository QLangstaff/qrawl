#![cfg(test)]
use clap::Parser;

use super::{run_command, Cli, Commands};
use crate::test_site::{listing_page, Reply, TestSite};
use crate::types::Context;

fn parse(args: &[&str]) -> Cli {
    Cli::parse_from(std::iter::once("qrawl").chain(args.iter().copied()))
}

async fn output(command: Commands) -> String {
    let mut out = Vec::new();
    run_command(command, Context::fast(), &mut out).await;
    String::from_utf8(out).expect("utf-8 output")
}

#[test]
fn the_fast_flag_is_global_and_every_command_parses() {
    assert!(parse(&["--fast", "url", "https://example.com"]).fast);
    assert!(!parse(&["url", "https://example.com"]).fast);
    let cli = parse(&[
        "urls",
        "https://a.example",
        "https://b.example",
        "--depth",
        "2",
        "--limit",
        "5",
        "--include-schema",
        "Recipe",
    ]);
    match cli.command {
        Commands::Urls {
            urls,
            depth,
            limit,
            include_schemas,
            ..
        } => {
            assert_eq!(urls, ["https://a.example", "https://b.example"]);
            assert_eq!((depth, limit), (2, Some(5)));
            assert_eq!(include_schemas, ["Recipe"]);
        }
        _ => panic!("not the urls command"),
    }
    assert!(Cli::try_parse_from(["qrawl", "urls"]).is_err());
}

#[tokio::test]
async fn url_writes_the_pages_content_as_json() {
    let site = TestSite::serve(|path| {
        Reply::Html(listing_page(
            path,
            r#"<script type="application/ld+json">{"@type":"Recipe","name":"Soup"}</script>"#,
            &[],
        ))
    })
    .await;

    let out = output(Commands::Url {
        url: site.url("/soup"),
    })
    .await;
    let page: serde_json::Value = serde_json::from_str(&out).expect("json");
    assert_eq!(page["title"], "/soup");
    assert_eq!(page["structured"][0]["name"], "Soup");
    assert_eq!(
        page["url"],
        format!("https://127.0.0.1:{}/soup", site.port())
    );
}

#[tokio::test]
async fn jsonld_and_schemas_read_a_file() {
    let dir = std::env::temp_dir().join(format!("qrawl-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("page.html");
    std::fs::write(
        &file,
        listing_page(
            "file",
            r#"<script type="application/ld+json">{"@type":"Product","name":"Pan"}</script>"#,
            &[],
        ),
    )
    .expect("write");
    let input = file.to_string_lossy().into_owned();

    let jsonld = output(Commands::Jsonld {
        input: input.clone(),
    })
    .await;
    assert!(jsonld.contains(r#""name": "Pan""#), "{jsonld}");
    let schemas = output(Commands::Schemas { input }).await;
    assert_eq!(schemas.trim(), "[\n  \"Product\"\n]");
    std::fs::remove_dir_all(dir).expect("cleanup");
}

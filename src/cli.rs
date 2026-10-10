//! CLI

use clap::{Parser, Subcommand};
use std::io::Read;

use crate::types::{Context, Html};
use crate::{runtime, templates, tools};

#[derive(Parser)]
#[command(
    name = "qrawl",
    version,
    about = "Rust toolkit to crawl web data for AI agents"
)]
struct Cli {
    /// Fetch with a single Minimal attempt instead of the profile cascade
    #[arg(long, global = true)]
    fast: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Fetch a Page and Report Its Size
    Fetch {
        /// URL
        url: String,
    },

    /// Crawl Children URLs (the ones that fetch)
    Children {
        /// URL
        url: String,
    },

    /// Map Page URLs
    Page {
        /// URL
        url: String,
    },

    /// Scrape Body
    Body {
        /// URL, file path, or `-` for stdin
        input: String,
    },

    /// Transform Page to Markdown
    Markdown {
        /// URL, file path, or `-` for stdin
        input: String,
    },

    /// Scrape schema.org Structured Data (JSON-LD, Microdata, RDFa, Microformats)
    Jsonld {
        /// URL, file path, or `-` for stdin
        input: String,
    },

    /// Scrape Metadata
    Metadata {
        /// URL, file path, or `-` for stdin
        input: String,
    },

    /// Scrape & Extract a Page's Preview (title, text, author, image, video)
    Preview {
        /// URL
        url: String,
    },

    /// Scrape & Extract schema.org Types
    Schemas {
        /// URL, file path, or `-` for stdin
        input: String,
    },

    /// Check if a Page Includes a schema.org Type
    HasSchema {
        /// URL, file path, or `-` for stdin
        input: String,
        /// schema.org type to check for (e.g. Recipe, Product)
        schema_type: String,
    },

    /// A Page and Its Children That Carry a schema.org Type
    Schema {
        /// URL
        url: String,
        /// schema.org type to keep (e.g. Recipe, Product)
        schema_type: String,
    },

    /// Extract & Normalize Email Addresses Two Links from a URL
    Emails {
        /// URL
        url: String,
    },

    /// Extract & Normalize Phone Numbers
    Phones {
        /// URL, file path, or `-` for stdin
        input: String,
    },

    /// Crawl a Site, Extract & Normalize Social Links
    Socials {
        /// URL
        url: String,
    },

    /// Any URL → One `PageContent`
    Url {
        /// URL
        url: String,
    },

    /// Many URLs → a `PageContent` Each (concurrent, best-effort)
    Urls {
        /// URLs (space-separated)
        #[arg(required = true)]
        urls: Vec<String>,
        /// Crawl depth: 0 = just these URLs, N = follow children N levels (at most 3)
        #[arg(long, default_value_t = 0)]
        depth: usize,
        /// Stop after this many matching results (caps a wide/deep crawl)
        #[arg(long)]
        limit: Option<usize>,
        /// Prefetch allowlist domain (repeatable); empty = allow all
        #[arg(long = "allow")]
        allow_domains: Vec<String>,
        /// Prefetch blocklist domain (repeatable)
        #[arg(long = "block")]
        block_domains: Vec<String>,
        /// Restrict the crawl to this exact URL (repeatable); empty = allow all
        #[arg(long = "allow-url")]
        allow_urls: Vec<String>,
        /// Exclude this exact URL from the crawl (repeatable)
        #[arg(long = "block-url")]
        block_urls: Vec<String>,
        /// Keep only results with this schema.org type (repeatable); empty = any
        #[arg(long = "include-schema")]
        include_schemas: Vec<String>,
        /// Drop results with this schema.org type (repeatable)
        #[arg(long = "exclude-schema")]
        exclude_schemas: Vec<String>,
    },
}

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("Error: {}", message);
    std::process::exit(1);
}

fn print_json<T: serde::Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{}", json),
        Err(e) => fail(format!("serializing to JSON: {}", e)),
    }
}

async fn fetch(url: &str, ctx: Context) -> Html {
    ctx.scope(tools::fetch::fetch_strategy(url))
        .await
        .unwrap_or_else(|e| fail(format!("fetching {}: {}", url, e)))
}

/// Load HTML from stdin (`-`), a URL, or a file path.
async fn load(input: &str, ctx: Context) -> Html {
    if input == "-" {
        let mut html = String::new();
        std::io::stdin()
            .read_to_string(&mut html)
            .unwrap_or_else(|e| fail(format!("reading stdin: {}", e)));
        Html::new(html)
    } else if input.starts_with("http://") || input.starts_with("https://") {
        fetch(input, ctx).await
    } else {
        Html::new(
            std::fs::read_to_string(input)
                .unwrap_or_else(|e| fail(format!("reading file '{}': {}", input, e))),
        )
    }
}

fn urls_of(pages: Vec<(String, Html)>) -> Vec<String> {
    pages.into_iter().map(|(url, _html)| url).collect()
}

pub fn run() {
    let cli = Cli::parse();
    let ctx = if cli.fast {
        Context::fast()
    } else {
        Context::auto()
    };
    runtime::block_on(run_command(cli.command, ctx));
}

async fn run_command(command: Commands, ctx: Context) {
    match command {
        Commands::Fetch { url } => {
            eprintln!(
                "Fetching {}... (set RUST_LOG=qrawl=info for profile/timing)",
                url
            );
            let html = fetch(&url, ctx).await;
            eprintln!("✓ Fetched {} bytes", html.as_str().len());
        }

        Commands::Children { url } => {
            print_json(&urls_of(templates::qrawl_children(vec![url], ctx).await));
        }

        Commands::Page { url } => {
            let html = fetch(&url, ctx).await;
            print_json(&tools::map::map_page(&html, &url).await);
        }

        Commands::Body { input } => {
            print_json(&tools::scrape::scrape_body(&load(&input, ctx).await).await);
        }

        Commands::Markdown { input } => {
            // Markdown is terminal, LLM-ready text — print it raw, not JSON-quoted.
            let html = load(&input, ctx).await;
            println!("{}", tools::transform::transform_markdown(&html).await);
        }

        Commands::Jsonld { input } => {
            print_json(&tools::scrape::scrape_jsonld(&load(&input, ctx).await).await);
        }

        Commands::Metadata { input } => {
            print_json(&tools::scrape::scrape_metadata(&load(&input, ctx).await).await);
        }

        Commands::Preview { url } => {
            let metadata = tools::scrape::scrape_metadata(&fetch(&url, ctx).await).await;
            print_json(&tools::extract::extract_preview(&metadata));
        }

        Commands::Schemas { input } => {
            let jsonld = tools::scrape::scrape_jsonld(&load(&input, ctx).await).await;
            print_json(&tools::extract::extract_schema_types(&jsonld));
        }

        Commands::HasSchema { input, schema_type } => {
            let jsonld = tools::scrape::scrape_jsonld(&load(&input, ctx).await).await;
            print_json(&tools::classify::classify_has_schema_type(
                &jsonld,
                &schema_type,
            ));
        }

        Commands::Schema { url, schema_type } => {
            let ctx = ctx.with_depth(1).with_include_schemas(vec![schema_type]);
            let pages = templates::qrawl_many_urls(vec![url], ctx).await;
            print_json(&pages.into_iter().map(|page| page.url).collect::<Vec<_>>());
        }

        Commands::Emails { url } => {
            print_json(&templates::qrawl_emails(vec![url], ctx).await);
        }

        Commands::Phones { input } => {
            let phones = tools::extract::extract_phones(&load(&input, ctx).await).await;
            print_json(&tools::normalize::normalize_phones(&phones));
        }

        Commands::Socials { url } => {
            // Depth 2: the seed, its children, and their children.
            let pages = templates::qrawl_many_urls(vec![url], ctx.with_depth(2)).await;
            let socials: Vec<String> = pages.into_iter().flat_map(|page| page.socials).collect();
            print_json(&tools::normalize::normalize_urls(&socials));
        }

        Commands::Url { url } => match templates::qrawl_any_url(&url, ctx).await {
            Ok(page) => print_json(&page),
            Err(e) => fail(e),
        },

        Commands::Urls {
            urls,
            depth,
            limit,
            allow_domains,
            block_domains,
            allow_urls,
            block_urls,
            include_schemas,
            exclude_schemas,
        } => {
            let ctx = ctx
                .with_depth(depth)
                .with_limit(limit)
                .with_allow_domains(allow_domains)
                .with_block_domains(block_domains)
                .with_allow_urls(allow_urls)
                .with_block_urls(block_urls)
                .with_include_schemas(include_schemas)
                .with_exclude_schemas(exclude_schemas);
            print_json(&templates::qrawl_many_urls(urls, ctx).await);
        }
    }
}

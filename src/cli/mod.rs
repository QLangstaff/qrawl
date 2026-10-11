//! CLI

mod tests;

use clap::{Parser, Subcommand};
use std::io::{Read, Write};

use crate::templates::{qrawl_any_url, qrawl_many_urls, CrawlOptions};
use crate::tools::fetch::Fetched;
use crate::types::{Context, Html};
use crate::{runtime, tools};

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

    /// Map Children URLs
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

    /// Any URL → Its Preview (title, text, author, image, video)
    Preview {
        /// URL
        url: String,
    },

    /// Scrape & Extract schema.org Types
    Schemas {
        /// URL, file path, or `-` for stdin
        input: String,
    },

    /// Check if a Page Includes a schema.org Type, at Any Depth
    HasSchema {
        /// URL, file path, or `-` for stdin
        input: String,
        /// schema.org type to check for (e.g. Recipe, Product)
        schema_type: String,
    },

    /// A Page and Its Children That Carry a schema.org Type, at Any Depth
    Schema {
        /// URL
        url: String,
        /// schema.org type to keep (e.g. Recipe, Product)
        schema_type: String,
    },

    /// Extract & Normalize Email Addresses
    Emails {
        /// URL, file path, or `-` for stdin
        input: String,
    },

    /// Extract & Normalize Phone Numbers
    Phones {
        /// URL, file path, or `-` for stdin
        input: String,
    },

    /// Extract & Normalize Social Links from a URL and Two Levels of Children
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
        /// Keep only results with this schema.org type at any depth (repeatable); empty = any
        #[arg(long = "include-schema")]
        include_schemas: Vec<String>,
        /// Drop results with this schema.org type at any depth (repeatable)
        #[arg(long = "exclude-schema")]
        exclude_schemas: Vec<String>,
    },
}

fn fail(message: impl std::fmt::Display) -> ! {
    eprintln!("Error: {}", message);
    std::process::exit(1);
}

fn write_json<T: serde::Serialize>(out: &mut impl Write, value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => writeln!(out, "{}", json).unwrap_or_else(|e| fail(e)),
        Err(e) => fail(format!("serializing to JSON: {}", e)),
    }
}

fn is_http_url(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Fetch `url`, over `https` when it names no scheme.
async fn fetch(url: &str, ctx: Context) -> Fetched {
    let address = tools::normalize::absolute_url(url)
        .unwrap_or_else(|| fail(format!("not an HTTP URL: {}", url)));
    ctx.scope(tools::fetch::fetch_strategy(&address))
        .await
        .unwrap_or_else(|e| fail(format!("fetching {}: {}", address, e)))
}

/// Load HTML from stdin (`-`), a URL, or a file path.
async fn load(input: &str, ctx: Context) -> Html {
    if input == "-" {
        let mut html = String::new();
        std::io::stdin()
            .read_to_string(&mut html)
            .unwrap_or_else(|e| fail(format!("reading stdin: {}", e)));
        Html::new(html)
    } else if is_http_url(input) {
        fetch(input, ctx).await.html
    } else {
        Html::new(
            std::fs::read_to_string(input)
                .unwrap_or_else(|e| fail(format!("reading file '{}': {}", input, e))),
        )
    }
}

pub fn run() {
    let cli = Cli::parse();
    let ctx = if cli.fast {
        Context::fast()
    } else {
        Context::auto()
    };
    runtime::block_on(run_command(cli.command, ctx, &mut std::io::stdout()));
}

/// Run `command`, writing its result to `out`; progress goes to stderr.
async fn run_command(command: Commands, ctx: Context, out: &mut (impl Write + Send)) {
    match command {
        Commands::Fetch { url } => {
            eprintln!(
                "Fetching {}... (set RUST_LOG=qrawl=info for profile/timing)",
                url
            );
            let fetched = fetch(&url, ctx).await;
            eprintln!(
                "✓ Fetched {} bytes from {}",
                fetched.html.as_str().len(),
                fetched.url
            );
        }

        Commands::Children { url } => {
            let fetched = fetch(&url, ctx).await;
            write_json(
                out,
                &tools::map::map_children(&fetched.html, &fetched.url).await,
            );
        }

        Commands::Page { url } => {
            let fetched = fetch(&url, ctx).await;
            write_json(
                out,
                &tools::map::map_page(&fetched.html, &fetched.url).await,
            );
        }

        Commands::Body { input } => {
            write_json(
                out,
                &tools::scrape::scrape_body(&load(&input, ctx).await).await,
            );
        }

        Commands::Markdown { input } => {
            // Markdown is terminal, LLM-ready text — print it raw, not JSON-quoted.
            let html = load(&input, ctx).await;
            writeln!(out, "{}", tools::transform::transform_markdown(&html).await)
                .unwrap_or_else(|e| fail(e));
        }

        Commands::Jsonld { input } => {
            write_json(
                out,
                &tools::scrape::scrape_jsonld(&load(&input, ctx).await).await,
            );
        }

        Commands::Metadata { input } => {
            write_json(
                out,
                &tools::scrape::scrape_metadata(&load(&input, ctx).await).await,
            );
        }

        Commands::Preview { url } => match qrawl_any_url(&url, ctx).await {
            Ok(page) => write_json(out, &tools::extract::extract_preview(&page.metadata)),
            Err(e) => fail(e),
        },

        Commands::Schemas { input } => {
            let jsonld = tools::scrape::scrape_jsonld(&load(&input, ctx).await).await;
            write_json(out, &tools::extract::extract_schema_types(&jsonld));
        }

        Commands::HasSchema { input, schema_type } => {
            let jsonld = tools::scrape::scrape_jsonld(&load(&input, ctx).await).await;
            write_json(
                out,
                &tools::classify::classify_has_schema_type(&jsonld, &schema_type),
            );
        }

        Commands::Schema { url, schema_type } => {
            let options = CrawlOptions::default()
                .with_depth(1)
                .with_include_schemas(vec![schema_type]);
            let pages = qrawl_many_urls(vec![url], options, ctx).await;
            write_json(
                out,
                &pages
                    .into_iter()
                    .map(|page| page.address)
                    .collect::<Vec<_>>(),
            );
        }

        Commands::Emails { input } => {
            let emails = tools::extract::extract_emails(&load(&input, ctx).await).await;
            write_json(out, &tools::normalize::normalize_emails(&emails));
        }

        Commands::Phones { input } => {
            let phones = tools::extract::extract_phones(&load(&input, ctx).await).await;
            write_json(out, &tools::normalize::normalize_phones(&phones));
        }

        Commands::Socials { url } => {
            let options = CrawlOptions::default().with_depth(2);
            let pages = qrawl_many_urls(vec![url], options, ctx).await;
            let socials: Vec<String> = pages.into_iter().flat_map(|page| page.socials).collect();
            write_json(out, &tools::normalize::normalize_urls(&socials));
        }

        Commands::Url { url } => match qrawl_any_url(&url, ctx).await {
            Ok(page) => write_json(out, &page),
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
                .with_allow_domains(allow_domains)
                .with_block_domains(block_domains);
            let options = CrawlOptions::default()
                .with_depth(depth)
                .with_allow_urls(allow_urls)
                .with_block_urls(block_urls)
                .with_include_schemas(include_schemas)
                .with_exclude_schemas(exclude_schemas);
            let options = match limit {
                Some(limit) => options.with_limit(limit),
                None => options,
            };
            write_json(out, &qrawl_many_urls(urls, options, ctx).await);
        }
    }
}

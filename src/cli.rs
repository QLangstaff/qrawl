//! CLI

use clap::{Parser, Subcommand};
use std::io::{self, Read};
use std::sync::Arc;

use crate::{runtime, templates, tools, types};

#[derive(Parser)]
#[command(
    name = "qrawl",
    version,
    about = "Rust toolkit to crawl web data for AI agents"
)]
struct Cli {
    #[arg(long, global = true)]
    fast: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Fetch Auto
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
        /// URL
        url: String,
    },

    /// Transform Page to Markdown
    Markdown {
        /// URL
        url: String,
    },

    /// Scrape JSON-LD
    Jsonld {
        /// URL
        url: String,
    },

    /// Scrape Metadata
    Metadata {
        /// URL
        url: String,
    },

    /// Scrape & Extract Open Graph Preview
    Preview {
        /// URL
        url: String,
    },

    /// Scrape & Extract JSON-LD Schema Types
    Schemas {
        /// URL
        url: String,
    },

    /// Check if a Page Includes a schema.org Type
    HasSchema {
        /// URL
        url: String,
        /// schema.org type to check for (e.g. Recipe, Product)
        schema_type: String,
    },

    /// Crawl a Site, Keep Pages of a schema.org Type
    Schema {
        /// URL
        url: String,
        /// schema.org type to keep (e.g. Recipe, Product)
        schema_type: String,
    },

    /// Extract & Normalize Email Addresses
    Emails {
        /// URL
        url: String,
    },

    /// Extract & Normalize Phone Numbers
    Phones {
        /// URL
        url: String,
    },

    /// Extract & Normalize Social Links
    Socials {
        /// URL
        url: String,
    },

    /// Universal URL Pipeline (classify → side-door or fetch+scrape → one bundle)
    Url {
        /// URL
        url: String,
    },

    /// Universal Bulk Pipeline (fast, concurrent, best-effort over many URLs)
    Urls {
        /// URLs (space-separated)
        urls: Vec<String>,
        /// Crawl depth ceiling: 0 = just these URLs, N = follow children up to N levels
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

pub fn read_input(input: &str, ctx: Arc<types::Context>) -> String {
    if input == "-" {
        // Read from stdin
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .expect("Failed to read from stdin");
        buffer
    } else if input.starts_with("http://") || input.starts_with("https://") {
        // Fetch from URL
        fetch_url(input, ctx)
    } else {
        // Read from file
        std::fs::read_to_string(input).unwrap_or_else(|e| {
            eprintln!("Error reading file '{}': {}", input, e);
            std::process::exit(1);
        })
    }
}

pub fn fetch_url(url: &str, ctx: Arc<types::Context>) -> String {
    let result = runtime::block_on(async move {
        types::CTX
            .scope(ctx, async { tools::fetch::fetch_strategy(url).await })
            .await
    });
    result
        .unwrap_or_else(|e| {
            eprintln!("Failed to fetch {}: {}", url, e);
            std::process::exit(1);
        })
        .into_inner()
}

pub fn print_json<T: serde::Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("Error serializing to JSON: {}", e),
    }
}

pub fn run() {
    let cli = Cli::parse();
    let ctx_arc = Arc::new(if cli.fast {
        types::Context::fast()
    } else {
        types::Context::auto()
    });

    match cli.command {
        Commands::Fetch { url } => {
            if !url.starts_with("https://") {
                eprintln!("Error: URL must start with https://");
                std::process::exit(1);
            }

            eprintln!(
                "Fetching {}... (set RUST_LOG=qrawl=info for profile/timing)",
                url
            );

            match runtime::block_on(tools::fetch::fetch_auto(&url)) {
                Ok(html) => eprintln!("✓ Fetched {} bytes", html.as_str().len()),
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Children { url } => {
            let result = runtime::block_on(templates::qrawl_children(
                vec![url.to_string()],
                (*ctx_arc).clone(),
            ));

            // Extract only URLs from the (URL, HTML) tuples
            let urls = result.map(|tuples| {
                tuples
                    .into_iter()
                    .map(|(url, _html)| url)
                    .collect::<Vec<String>>()
            });

            print_json(&urls);
        }

        Commands::Page { url } => {
            run!(@async ctx_arc.clone(), url.clone(), tools::map::map_page, &url)
        }

        Commands::Body { url } => {
            run!(@async ctx_arc.clone(), url, tools::scrape::scrape_body)
        }

        Commands::Markdown { url } => {
            // Markdown is terminal, LLM-ready text — print it raw, not JSON-quoted.
            let html = read_input(&url, ctx_arc.clone());
            let markdown = runtime::block_on(tools::transform::transform_markdown(
                &types::Html::new(html),
            ));
            println!("{}", markdown);
        }

        Commands::Jsonld { url } => {
            run!(@async ctx_arc.clone(), url, tools::scrape::scrape_jsonld)
        }

        Commands::Metadata { url } => {
            run!(@async ctx_arc.clone(), url, tools::scrape::scrape_metadata)
        }

        Commands::Preview { url } => {
            // `extract_open_graph` needs the URL (it carries it into the result), so this
            // composes by hand rather than via the single-input `run!` form:
            // fetch → scrape_metadata → extract_open_graph.
            let canonical = types::CanonicalUrl::new(&url);
            let html = read_input(&url, ctx_arc.clone());
            let result = runtime::block_on(async move {
                let metadata = tools::scrape::scrape_metadata(&types::Html::new(html)).await;
                tools::extract::extract_open_graph(&canonical, &metadata)
            });
            print_json(&result);
        }

        Commands::Schemas { url } => run!(
            @async ctx_arc.clone(), url,
            [
                tools::scrape::scrape_jsonld,
                tools::extract::extract_schema_types
            ]
        ),

        Commands::HasSchema { url, schema_type } => {
            // Two inputs (URL + type), so this composes by hand rather than via
            // the single-input `run!` form: fetch → scrape_jsonld → classify.
            let html = read_input(&url, ctx_arc.clone());
            let result = runtime::block_on(async move {
                let jsonld = tools::scrape::scrape_jsonld(&types::Html::new(html)).await;
                tools::classify::classify_has_schema_type(&jsonld, &schema_type)
            });
            print_json(&result);
        }

        Commands::Schema { url, schema_type } => {
            let result = runtime::block_on(templates::qrawl_schema(
                vec![url.to_string()],
                &schema_type,
                (*ctx_arc).clone(),
            ));
            // Print just the matching page URLs (like `children`).
            let urls = result.map(|pages| {
                pages
                    .into_iter()
                    .map(|(url, _html)| url)
                    .collect::<Vec<String>>()
            });
            print_json(&urls);
        }

        Commands::Emails { url } => {
            run!(@template url, templates::qrawl_emails, (*ctx_arc).clone())
        }

        Commands::Phones { url } => run!(
            @async ctx_arc.clone(), url,
            [tools::extract::extract_phones, tools::normalize::normalize_phones]
        ),

        Commands::Socials { url } => {
            // The universal bulk pipeline fills `PageContent::socials` on every
            // page; depth 2 mirrors the old seed → children → pages sweep.
            let mut ctx = (*ctx_arc).clone();
            ctx.depth = 2;
            let pages = runtime::block_on(templates::qrawl_many_urls(vec![url], ctx));
            let all: Vec<String> = pages.into_iter().flat_map(|p| p.socials).collect();
            print_json(&tools::normalize::normalize_social_urls(&all));
        }

        Commands::Url { url } => {
            // `qrawl_any_url` takes the URL string and fetches internally, so
            // pass it straight through — do NOT pre-fetch it to HTML.
            let result = runtime::block_on(templates::qrawl_any_url(&url, (*ctx_arc).clone()));
            print_json(&result);
        }

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
            // Bulk, concurrent, best-effort: one bundle per matching URL. `--fast`
            // toggles strategy; depth/results/domains/schemas/urls shape the qrawl.
            let mut ctx = (*ctx_arc).clone();
            ctx.depth = depth;
            ctx.limit = limit;
            ctx.allow_domains = allow_domains;
            ctx.block_domains = block_domains;
            ctx.allow_urls = allow_urls;
            ctx.block_urls = block_urls;
            ctx.include_schemas = include_schemas;
            ctx.exclude_schemas = exclude_schemas;
            let result = runtime::block_on(templates::qrawl_many_urls(urls, ctx));
            print_json(&result);
        }
    }
}

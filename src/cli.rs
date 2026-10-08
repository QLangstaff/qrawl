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
    /// Fetch Auto (HTML to stdout, status to stderr)
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

    /// Extract & Clean Email Addresses
    Emails {
        /// URL
        url: String,
    },

    /// Extract & Clean Phone Numbers
    Phones {
        /// URL
        url: String,
    },

    /// Fetch & Extract Products (name, price, images, attributes, …)
    Products {
        /// Product or listing page URLs; `-` reads URLs from stdin (any text or
        /// JSON containing http(s) URLs, e.g. the output of `qrawl children`)
        #[arg(required = true)]
        urls: Vec<String>,

        /// Treat URLs as listing pages (category / search results): extract
        /// products from the pages they link to, plus any listed on the page
        #[arg(long)]
        children: bool,

        /// With --children, the maximum number of child pages to read (0 = all)
        #[arg(long, default_value_t = 50)]
        limit: usize,
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

/// URLs from arguments; a lone `-` reads every http(s) URL found in stdin.
fn read_urls(args: Vec<String>) -> Vec<String> {
    if args != ["-"] {
        return args;
    }
    let mut buffer = String::new();
    io::stdin()
        .read_to_string(&mut buffer)
        .expect("Failed to read from stdin");
    buffer
        .split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | '[' | ']'))
        .filter(|t| t.starts_with("http://") || t.starts_with("https://"))
        .map(str::to_string)
        .collect()
}

pub fn fetch_url(url: &str, ctx: Arc<types::Context>) -> String {
    let result = runtime::block_on(async move {
        types::CTX
            .scope(ctx, async { tools::fetch::fetch_strategy(url).await })
            .await
    });
    result.map(types::Html::into_inner).unwrap_or_else(|e| {
        eprintln!("Failed to fetch {}: {}", url, e);
        std::process::exit(1);
    })
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

            eprintln!("Fetching {}...", url);

            match runtime::block_on(tools::fetch::fetch_auto_with_result(&url)) {
                Ok(result) => {
                    eprintln!(
                        "✓ Success\n  Profile: {:?}\n  Attempts: {}\n  Duration: {}ms",
                        result.profile_used, result.attempts, result.duration_ms
                    );
                    // The HTML goes to stdout so it can be saved or piped
                    // (`qrawl fetch URL | qrawl jsonld -`).
                    println!("{}", result.html);
                }
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

        Commands::Jsonld { url } => {
            run!(@async ctx_arc.clone(), url, tools::scrape::scrape_jsonld)
        }

        Commands::Metadata { url } => {
            run!(@async ctx_arc.clone(), url, tools::scrape::scrape_metadata)
        }

        Commands::Preview { url } => run!(
            @async ctx_arc.clone(), url,
            [
                tools::scrape::scrape_metadata,
                tools::extract::extract_og_preview
            ]
        ),

        Commands::Schemas { url } => run!(
            @async ctx_arc.clone(), url,
            [
                tools::scrape::scrape_jsonld,
                tools::extract::extract_schema_types
            ]
        ),

        Commands::Emails { url } => {
            run!(@template url, templates::qrawl_emails, (*ctx_arc).clone())
        }

        Commands::Phones { url } => run!(
            @async ctx_arc.clone(), url,
            [tools::extract::extract_phones, tools::normalize::normalize_phones]
        ),

        Commands::Products {
            urls,
            children,
            limit,
        } => {
            let urls = read_urls(urls);
            let ctx = (*ctx_arc).clone();
            let pages = runtime::block_on(async move {
                if children {
                    // Listing pages can carry products themselves (`ItemList`).
                    let mut pages: Vec<_> = templates::qrawl_products(urls.clone(), ctx.clone())
                        .await
                        .into_iter()
                        .filter(|p| !p.products.is_empty() || p.error.is_some())
                        .collect();
                    pages.extend(templates::qrawl_child_products(urls, ctx, limit).await);
                    // A listing with no detectable children falls back to itself
                    // (in canonical form, so compare canonically).
                    let mut seen = std::collections::HashSet::new();
                    pages.retain(|p| seen.insert(tools::normalize::normalize_social(&p.url)));
                    pages
                } else {
                    templates::qrawl_products(urls, ctx).await
                }
            });
            print_json(&pages);
        }
    }
}

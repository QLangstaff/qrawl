//! CLI

use clap::{Parser, Subcommand};
use std::io::{self, Read};

use crate::runtime;
use crate::tools::fetch::fetch_auto;

#[derive(Parser)]
#[command(
    name = "qrawl",
    version,
    about = "Composable web crawling tools for Rust"
)]
struct Cli {
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
}

pub fn read_input(input: &str) -> String {
    if input == "-" {
        // Read from stdin
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .expect("Failed to read from stdin");
        buffer
    } else if input.starts_with("http://") || input.starts_with("https://") {
        // Fetch from URL
        fetch_url(input)
    } else {
        // Read from file
        std::fs::read_to_string(input).unwrap_or_else(|e| {
            eprintln!("Error reading file '{}': {}", input, e);
            std::process::exit(1);
        })
    }
}

pub fn fetch_url(url: &str) -> String {
    runtime::block_on(fetch_auto(url)).unwrap_or_else(|e| {
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
    use crate::tools;

    let cli = Cli::parse();

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
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Commands::Children { url } => {
            run!(
                @async url.clone(),
                tools::map::map_children,
                &url
            )
        }

        Commands::Page { url } => {
            run!(@async url.clone(), tools::map::map_page, &url)
        }

        Commands::Body { url } => {
            run!(@async url, tools::scrape::scrape_body)
        }

        Commands::Jsonld { url } => {
            run!(@async url, tools::scrape::scrape_jsonld)
        }

        Commands::Metadata { url } => {
            run!(@async url, tools::scrape::scrape_metadata)
        }

        Commands::Preview { url } => run!(
            @async url,
            [
                tools::scrape::scrape_metadata,
                tools::extract::extract_og_preview
            ]
        ),

        Commands::Schemas { url } => run!(
            @async url,
            [
                tools::scrape::scrape_jsonld,
                tools::extract::extract_schema_types
            ]
        ),

        Commands::Emails { url } => run!(
            @async_chain url,
            [tools::extract::extract_emails, tools::clean::clean_emails]
        ),

        Commands::Phones { url } => run!(
            @async_chain url,
            [tools::extract::extract_phones, tools::clean::clean_phones]
        ),
    }
}

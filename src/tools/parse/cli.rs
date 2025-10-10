/// CLI for the parse tool.
use crate::tools::parse::{parse, parse_children, parse_siblings, ParseOptions};
use clap::{Parser, Subcommand};
use std::io::{self, Read};

#[derive(Parser)]
#[command(
    name = "parse",
    about = "Parse HTML into structured sections and blocks"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Parse clean HTML (no main extraction)
    Clean { input: String },
    /// Parse main content area (clean + main)
    Main { input: String },
    /// Parse siblings (clean + main)
    Siblings { input: String },
    /// Parse children/links from siblings (clean + main)
    Children { input: String },
}

pub fn run() {
    let cli = Cli::parse();

    match cli.cmd {
        Some(Command::Clean { input }) => {
            let html = read_input(&input);
            let opts = ParseOptions {
                clean: true,
                main: false,
                ..Default::default()
            };
            let result = parse(&html, &opts);
            print_json(&result);
        }
        Some(Command::Main { input }) => {
            let html = read_input(&input);
            let result = parse(&html, &ParseOptions::default());
            print_json(&result);
        }
        Some(Command::Siblings { input }) => {
            let html = read_input(&input);
            let result = parse_siblings(&html, &ParseOptions::default());
            print_json(&result);
        }
        Some(Command::Children { input }) => {
            let html = read_input(&input);
            let result = parse_children(&html, &ParseOptions::default());
            print_json(&result);
        }
        None => {
            eprintln!("Usage: parse <COMMAND>");
            eprintln!("Run 'parse --help' for more information");
        }
    }
}

fn read_input(input: &str) -> String {
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

fn fetch_url(url: &str) -> String {
    use crate::tools::fetch::fetch;

    let runtime = tokio::runtime::Runtime::new().expect("Failed to create async runtime");
    runtime
        .block_on(fetch(url))
        .unwrap_or_else(|e| {
            eprintln!("Failed to fetch {}: {}", url, e);
            std::process::exit(1);
        })
        .html
}

fn print_json<T: serde::Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("Error serializing to JSON: {}", e),
    }
}

/// CLI for the fetch tool.
use crate::tools::fetch::fetch_auto;
use clap::Parser;

#[derive(Parser)]
#[command(
    name = "fetch",
    about = "Fetch HTML from URLs with adaptive bot evasion"
)]
struct Cli {
    /// URL to fetch
    url: String,
}

pub fn run() {
    let cli = Cli::parse();
    run_with_args(cli.url);
}

// Exposed function for delegation from unified CLI

pub fn run_with_args(url: String) {
    // Validate URL
    if !url.starts_with("http://") && !url.starts_with("https://") {
        eprintln!("Error: URL must start with http:// or https://");
        std::process::exit(1);
    }

    // Create async runtime and execute
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create async runtime");

    eprintln!("Fetching {}...", url);

    let result = runtime.block_on(fetch_auto(&url));

    match result {
        Ok(html) => {
            // Output HTML content to stdout
            println!("{}", html);
            eprintln!("✓ Fetched successfully");
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

# qrawl

**Crawl the web for AI agents — in Rust.**

[![Crates.io](https://img.shields.io/crates/v/qrawl.svg)](https://crates.io/crates/qrawl)
[![Downloads](https://img.shields.io/crates/d/qrawl.svg)](https://crates.io/crates/qrawl)
[![docs.rs](https://img.shields.io/docsrs/qrawl)](https://docs.rs/qrawl)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/QLangstaff/qrawl/blob/main/LICENSE)
[![Rust 1.88+](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)

Your agent's data source is the open web — but a plain `GET` returns a `403`, raw HTML burns your context window on `<div>` soup, and the signal you actually want (the content, the emails, the structured fields) is buried in markup.

**qrawl fetches *past* the blocks, strips the markup, and extracts the signal** — as small, composable, async tools that snap together into one pipeline. Use it as a Rust library, or as a CLI that hands your agent clean JSON.

## Try it

```bash
cargo install qrawl
qrawl url "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
```

```json
{
  "url": "https://youtube.com/watch?v=dQw4w9WgXcQ",
  "platform": "YouTube",
  "kind": "Video",
  "source": "Oembed",
  "title": "Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)",
  "text": null,
  "author": "Rick Astley",
  "imageUrl": "https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg",
  "videoUrl": null,
  "structured": [],
  "metadata": [],
  "socials": [],
  "body": null
}
```

`url` takes any link and returns one shape, `PageContent`. A recognized social item (Reddit, TikTok, YouTube, Pinterest, Vimeo) goes through its *side door* — Reddit's `.json` or the platform's oEmbed. Everything else is fetched — retrying across browser profiles (Minimal → Windows → iOS) if a site pushes back — and scraped for schema.org data, `<meta>` tags, social links, and body HTML. No API key, no headless browser, no config.

## Compose them

Tools are plain functions — chain them with normal Rust:

```bash
cargo add qrawl
cargo add tokio -F full
```

```rust,no_run
use qrawl::tools::fetch::fetch_auto;
use qrawl::tools::transform::transform_markdown;

#[tokio::main]
async fn main() {
    // HTTP that gets through (retry across browser profiles), rendered to
    // LLM-ready Markdown.
    let html = fetch_auto("https://www.rust-lang.org").await.unwrap();
    let markdown = transform_markdown(&html).await;
    println!("{markdown}");
}
```

Or declare a whole crawl with `chain!`, which fans each step out with bounded concurrency:

```rust,no_run
use qrawl::chain;
use qrawl::tools::extract::extract_emails;
use qrawl::tools::map::map_children;
use qrawl::tools::normalize::{normalize_emails, normalize_urls};
use qrawl::types::Context;

async fn emails(urls: Vec<String>) -> Vec<String> {
    chain! {
        urls, Context::auto() =>
        all(normalize_urls) -> fetch -> map(map_children) ->
        all(normalize_urls) -> fetch -> extract(extract_emails) -> all(normalize_emails)
    }
}
```

A library that doesn't need the CLI can drop `clap` with `default-features = false`.

## The toolkit

Small, focused functions — async where they fetch or parse, sync where they don't.

| Tool | What it does |
|------|--------------|
| **fetch** | HTTP that gets through — auto-retry across browser profiles, or a fast single attempt; social side doors |
| **scrape** | Pull the body, `<meta>` tags, and schema.org data (JSON-LD, Microdata, RDFa, Microformats — merged) out of HTML |
| **extract** | Surface structured signal: emails, phones, social links, schema.org types, Open Graph content |
| **map** | Discover URLs from a page: every link, or just the children |
| **classify** | Answer yes/no: is this URL social, does this page carry a schema.org type |
| **normalize** | Canonicalize URLs, strip junk HTML, normalize text, dedupe emails / phones / socials |
| **transform** | Render HTML to Markdown |
| **batch** | Run any async op across a `Vec` with bounded concurrency |

## Templates: pre-built pipelines

Drop-in compositions of the tools for the common jobs. They're tiny — [read the source](https://github.com/QLangstaff/qrawl/tree/main/src/templates).

| Template | Give it | Get back |
|----------|---------|----------|
| `qrawl_any_url` | any link | one `PageContent` (the `url` command above) |
| `qrawl_many_urls` | many links | a `PageContent` per page, optionally crawled `depth` levels deep and filtered by domain, URL, or schema.org type |
| `qrawl_share` | a shared link and its caption | a `PageContent` that degrades to the caption when nothing fetches |
| `qrawl_children` | parent URLs | the fetched HTML of each child page |
| `qrawl_children_stream`, `qrawl_discover_children`, `qrawl_fetch_stream` | parent URLs | the same crawl as streams, to filter or stop early |
| `qrawl_schema` | site URLs and a schema.org type | the child pages that carry that type |
| `qrawl_emails` | site URLs | the deduped emails on the pages two links from each URL |

```rust,no_run
use qrawl::templates::qrawl_many_urls;
use qrawl::types::{Context, PageContent};

// One level deep, recipe pages only, stop after 30.
async fn recipes(urls: Vec<String>) -> Vec<PageContent> {
    let ctx = Context::fast()
        .with_depth(1)
        .with_limit(30)
        .with_include_schemas(vec!["Recipe".to_string()]);
    qrawl_many_urls(urls, ctx).await
}
```

Full API reference: [docs.rs/qrawl](https://docs.rs/qrawl).

## CLI reference

Each command prints JSON to stdout, except `markdown` (raw Markdown) and `fetch` (a byte count on stderr). A command that can't read its input or fetch its URL fails with a message on stderr and exit code 1; a crawl drops the pages that fail and prints what it got. Add `--fast` for a single fetch attempt instead of the retry cascade. Set `RUST_LOG=qrawl=info` to log each fetch's winning profile and timing.

Commands marked `<input>` take a URL, a file path, or `-` for stdin.

| Command | Output |
|---------|--------|
| `qrawl url <url>` | any link → `PageContent` |
| `qrawl urls <url>...` | many links → a `PageContent` each; `--depth` (at most 3), `--limit`, `--allow`, `--block`, `--allow-url`, `--block-url`, `--include-schema`, `--exclude-schema` |
| `qrawl fetch <url>` | fetch and report the page's size |
| `qrawl preview <url>` | Open Graph content — title, text, author, image, video |
| `qrawl metadata <input>` | all `<meta>` tags |
| `qrawl body <input>` | the `<body>` HTML |
| `qrawl markdown <input>` | the page as Markdown |
| `qrawl jsonld <input>` | schema.org data — JSON-LD, Microdata, RDFa, and Microformats, merged |
| `qrawl schemas <input>` | schema.org types on the page |
| `qrawl has-schema <input> <type>` | whether the page carries a schema.org type |
| `qrawl phones <input>` | phone numbers |
| `qrawl page <url>` | every link on the page |
| `qrawl children <url>` | child URLs under the page that fetch |
| `qrawl schema <url> <type>` | child pages that carry a schema.org type |
| `qrawl emails <url>` | deduped emails on the pages two links from the URL |
| `qrawl socials <url>` | crawl the site, return deduped social links |

## License

MIT — see [LICENSE](https://github.com/QLangstaff/qrawl/blob/main/LICENSE).

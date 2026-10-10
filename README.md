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
  "address": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
  "platform": "YouTube",
  "kind": "Video",
  "handle": null,
  "id": "dQw4w9WgXcQ",
  "title": "Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)",
  "text": "The official video for “Never Gonna Give You Up” by Rick Astley. …",
  "author": "Rick Astley",
  "imageUrl": "https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg",
  "videoUrl": "https://www.youtube.com/embed/dQw4w9WgXcQ",
  "structured": ["… schema.org entities …"],
  "metadata": [["oembed:title", "…"], ["og:title", "…"], "…"],
  "socials": [],
  "body": "<body>…</body>"
}
```

`url` takes any link — a blog post, a store page, a TikTok video — and returns one shape, `PageContent`, filled the same way every time. It fetches the page, retrying across browser profiles (Minimal → Windows → iOS) if a site pushes back, plus the item's embed when the platform serves one (Reddit's `.json`, oEmbed for TikTok, YouTube, Pinterest, and Vimeo). It scrapes the page into schema.org data, metadata, social links, and body HTML, and the embed into more metadata, which wins for the title, author, and image. You map that shape to your own domain. No API key, no headless browser, no config.

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
    let page = fetch_auto("https://www.rust-lang.org").await.unwrap();
    let markdown = transform_markdown(&page.html).await;
    println!("{markdown}");
}
```

Or declare a whole crawl with `chain!`, which fans each step out with bounded concurrency:

```rust,no_run
use qrawl::chain;
use qrawl::tools::extract::extract_emails;
use qrawl::tools::map::map_children;
use qrawl::tools::normalize::{dedupe_urls, normalize_emails};
use qrawl::types::Context;

async fn emails(urls: Vec<String>) -> Vec<String> {
    chain! {
        urls, Context::auto() =>
        all(dedupe_urls) -> fetch -> map(map_children) ->
        all(dedupe_urls) -> fetch -> extract(extract_emails) -> all(normalize_emails)
    }
}
```

A library that doesn't need the CLI can drop `clap` with `default-features = false`.

## The toolkit

Small, focused functions — async where they fetch or parse, sync where they don't. Each tool owns one job and never calls another; you chain them.

| Tool | What it does |
|------|--------------|
| **fetch** | HTTP that gets through — auto-retry across browser profiles, or a fast single attempt; a platform's embed of an item |
| **scrape** | Pull the body, `<meta>` tags, and schema.org data (JSON-LD, Microdata, RDFa, Microformats — merged) out of HTML, and metadata out of an embed |
| **extract** | Surface structured signal: emails, phones, schema.org types, a preview, a URL's social platform and ids |
| **map** | Discover URLs from a page: every link, or just the children |
| **classify** | Answer yes/no: does this page carry a schema.org type |
| **normalize** | Canonicalize and dedupe URLs, strip junk HTML, normalize text, dedupe emails / phones |
| **transform** | Render HTML to Markdown |
| **batch** | Run any async op across a `Vec` with bounded concurrency |

## Templates: any URL in, one shape out

Two compositions of the tools. They're small — [read the source](https://github.com/QLangstaff/qrawl/tree/main/src/templates).

| Template | Give it | Get back |
|----------|---------|----------|
| `qrawl_any_url` | any link | one `PageContent` (the `url` command above) |
| `qrawl_many_urls` | many links | a `PageContent` per page, optionally crawled up to 3 levels of children deep and filtered by URL or schema.org type |

```rust,no_run
use qrawl::templates::{qrawl_many_urls, CrawlOptions, PageContent};
use qrawl::types::Context;

// One level deep, recipe pages only, stop after 30.
async fn recipes(urls: Vec<String>) -> Vec<PageContent> {
    let options = CrawlOptions::default()
        .with_depth(1)
        .with_limit(30)
        .with_include_schemas(vec!["Recipe".to_string()]);
    qrawl_many_urls(urls, options, Context::fast()).await
}
```

`PageContent` is domain-agnostic. A recipe app reads `structured` for a schema.org `Recipe` and falls back to `title` and `text` for a video's caption; a product finder reads `structured` for `Product`. `Context` carries what every fetch honors: the strategy, the concurrency, and the domains to allow or block.

Full API reference: [docs.rs/qrawl](https://docs.rs/qrawl).

## CLI reference

Each command prints JSON to stdout, except `markdown` (raw Markdown) and `fetch` (a byte count on stderr). A command that can't read its input or fetch its URL fails with a message on stderr and exit code 1; a crawl drops the pages that fail and prints what it got. Add `--fast` for a single fetch attempt instead of the retry cascade. Set `RUST_LOG=qrawl=info` to log each fetch's winning profile and timing.

Commands marked `<input>` take a URL, a file path, or `-` for stdin.

| Command | Output |
|---------|--------|
| `qrawl url <url>` | any link → `PageContent` |
| `qrawl urls <url>...` | many links → a `PageContent` each; `--depth` (at most 3), `--limit`, `--allow`, `--block`, `--allow-url`, `--block-url`, `--include-schema`, `--exclude-schema` |
| `qrawl fetch <url>` | fetch and report the page's size and the URL that served it |
| `qrawl preview <url>` | any link → its preview: title, text, author, image, video |
| `qrawl metadata <input>` | all `<meta>` tags |
| `qrawl body <input>` | the `<body>` HTML |
| `qrawl markdown <input>` | the page as Markdown |
| `qrawl jsonld <input>` | schema.org data — JSON-LD, Microdata, RDFa, and Microformats, merged |
| `qrawl schemas <input>` | schema.org types on the page |
| `qrawl has-schema <input> <type>` | whether the page carries a schema.org type |
| `qrawl phones <input>` | phone numbers |
| `qrawl page <url>` | every link on the page |
| `qrawl children <url>` | the page's children: the items of its main listing |
| `qrawl schema <url> <type>` | the addresses of the page and its children that carry a schema.org type |
| `qrawl emails <input>` | email addresses |
| `qrawl socials <url>` | deduped social links on the page and two levels of its children |

## License

MIT — see [LICENSE](https://github.com/QLangstaff/qrawl/blob/main/LICENSE).

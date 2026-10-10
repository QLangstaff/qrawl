# qrawl

**Crawl the web for AI agents — in Rust.**

[![Crates.io](https://img.shields.io/crates/v/qrawl.svg)](https://crates.io/crates/qrawl)
[![Downloads](https://img.shields.io/crates/d/qrawl.svg)](https://crates.io/crates/qrawl)
[![docs.rs](https://img.shields.io/docsrs/qrawl)](https://docs.rs/qrawl)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/QLangstaff/qrawl/blob/main/LICENSE)
[![Rust 1.74+](https://img.shields.io/badge/rust-1.74%2B-orange.svg)](https://www.rust-lang.org)

Your agent's data source is the open web — but a plain `GET` returns a `403`, raw HTML burns your context window on `<div>` soup, and the signal you actually want (the content, the emails, the structured fields) is buried in markup.

**qrawl fetches *past* the blocks, strips the markup, and extracts the signal** — as small, composable, async tools that snap together into one pipeline. Use it as a Rust library, or as a CLI that hands your agent clean JSON.

## Try it

```bash
cargo install qrawl
qrawl preview https://www.rust-lang.org
```

```json
{
  "title": "Rust Programming Language",
  "description": "A language empowering everyone to build reliable and efficient software.",
  "image": "https://www.rust-lang.org/static/images/rust-social-wide.jpg"
}
```

That was a real fetch — with automatic retry across browser profiles (Minimal → Windows → iOS) if a site pushes back — scraped down to clean JSON. No API key, no headless browser, no config.

### Hand your agent any link → get structured content back

```bash
qrawl share "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
```

```json
{
  "Ok": {
    "url": "https://youtube.com/watch?v=dQw4w9WgXcQ",
    "platform": "YouTube",
    "kind": "Video",
    "title": "Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)",
    "text": null,
    "author": "Rick Astley",
    "image": "https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg",
    "media": null,
    "source": "Oembed",
    "raw": { "...": "full oEmbed payload (trimmed)" }
  }
}
```

`share` classifies the URL (Reddit, TikTok, Instagram, YouTube, X, LinkedIn, Pinterest, Vimeo, …) and routes it through the right *side door* — Reddit's `.json`, the platform's oEmbed, or Open Graph — to return one unified shape. Ideal for a share sheet, an inbox triager, or an agent making sense of a dropped link.

> The CLI prints the call's `Result`, so a success comes back under `"Ok"`. In the library you get the typed `Content` struct directly.

## Compose them

Tools are plain async functions — chain them with normal Rust:

```bash
cargo add qrawl
cargo add tokio -F full
```

```rust
use qrawl::tools::fetch::fetch_auto;
use qrawl::tools::normalize::normalize_html;

#[tokio::main]
async fn main() {
    // HTTP that gets through (retry across browser profiles), stripped to
    // readable content.
    let html = fetch_auto("https://www.rust-lang.org").await.unwrap();
    let clean = normalize_html(&html).await;
    println!("{} chars of clean content", clean.as_str().len());
}
```

For bounded-concurrency crawls over many URLs, reach for the [templates](#templates-pre-built-pipelines) below, or fan a tool out yourself with `qrawl::tools::batch::batch`.

## The toolkit

Small, focused functions — async where they touch the network, sync where they don't. Use them standalone, batch them, or compose them directly.

| Tool | What it does |
|------|--------------|
| **fetch** | HTTP that gets through — auto-retry across browser profiles, or a fast single-shot |
| **scrape** | Pull body content, `<meta>` tags, schema.org (JSON-LD, Microdata, RDFa), and Microformats (mf2 + mf1) out of any HTML |
| **extract** | Surface structured signal: emails, phones, schema.org types, OG previews |
| **map** | Discover URLs from a page: every link, just the children, or item-list siblings |
| **normalize** | Normalize URLs, strip HTML, normalize unicode, dedupe emails / phones / socials |
| **batch** | Run any async op across a `Vec` with bounded concurrency and backpressure |

## Templates: pre-built pipelines

Drop-in chains for the common jobs. They're tiny — [read the source](https://github.com/QLangstaff/qrawl/blob/main/src/templates/mod.rs).

| Template | Give it | Get back |
|----------|---------|----------|
| `qrawl_children` | a parent URL | clean fetched HTML for each child page |
| `qrawl_emails` | a site URL | every deduped email, crawled two levels deep |
| `qrawl_share` | any link | unified structured content (the `share` command above) |

```rust
use qrawl::templates::qrawl_emails;
use qrawl::types::Context;

let emails = qrawl_emails(urls, Context::auto()).await?;
```

Full API reference: [docs.rs/qrawl](https://docs.rs/qrawl).

## CLI reference

Each command takes a URL and prints JSON to stdout (except `fetch`, which reports the winning profile). Add `--fast` to skip the retry cascade for a single-shot fetch.

| Command | Output |
|---------|--------|
| `qrawl fetch <url>` | fetch and report which browser profile got through |
| `qrawl preview <url>` | Open Graph preview — title, description, image |
| `qrawl metadata <url>` | all `<meta>` tags |
| `qrawl body <url>` | readable body content |
| `qrawl jsonld <url>` | raw JSON-LD blocks |
| `qrawl microdata <url>` | Microdata items, JSON-LD-shaped |
| `qrawl rdfa <url>` | RDFa items, JSON-LD-shaped |
| `qrawl microformats <url>` | Microformats items (`h-card`/`h-entry`/…; mf1 upgraded to mf2) |
| `qrawl structured <url>` | all schema.org — JSON-LD + Microdata + RDFa, merged |
| `qrawl schemas <url>` | schema.org types on the page (all three encodings) |
| `qrawl page <url>` | every link on the page |
| `qrawl children <url>` | child URLs under the page |
| `qrawl emails <url>` | crawl the site, return deduped emails |
| `qrawl phones <url>` | phone numbers |
| `qrawl socials <url>` | crawl the site, return deduped social links |
| `qrawl share <url>` | any link → unified structured content |

## License

MIT — see [LICENSE](https://github.com/QLangstaff/qrawl/blob/main/LICENSE).

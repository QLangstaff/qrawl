# qrawl — Step 2 (minimal)

**Real fetching + scraping** wired in. No heuristics, no auto-follow. Clean hook points for future steps.

## Try it

```bash
# from this folder
cargo run --bin qrawl -- policy create example.com | jq
cargo run --bin qrawl -- extract https://example.com/ | jq
```

- CLI prints **pretty JSON**.
- `extract` auto-detects known vs unknown (creates a default policy if missing).
- Fan-out (follow links) is OFF by default; if you want it:
  1) set a policy area's `follow_links.enabled = true`
  2) in your code, set `EngineOptions.follow_depth = 1`

## Use as a library

```rust
use qrawl::{LocalFsStore, api::{self, Components}};

fn main() -> qrawl::Result<()> {
    let store = LocalFsStore::new()?;
    let components = Components::default(); // ReqwestFetcher + DefaultScraper
    let bundle = api::extract_url_auto(&store, "https://example.com/", &components)?;
    println!("{}", serde_json::to_string_pretty(&bundle)?);
    Ok(())
}
```

# Qrawl

Composable web crawling tools for Rust

## Install

```bash
cargo add qrawl
```

## Tools

- **[batch](src/tools/batch)**: Batch operations concurrently
- **[classify](src/tools/classify)**: Classify URLs and structured data
- **[extract](src/tools/extract)**: Extract structured data (emails, phones, previews, products)
- **[fetch](src/tools/fetch)**: Fetch with auto or fast strategy
- **[map](src/tools/map)**: Map URLs
- **[normalize](src/tools/normalize)**: Normalize raw data
- **[scrape](src/tools/scrape)**: Scrape raw data
- **[transform](src/tools/transform)**: Transform HTML to Markdown

## Templates

- **[qrawl_children](src/templates)**: Get children from parent URLs
- **[qrawl_emails](src/templates)**: Get emails from given URLs
- **[qrawl_products](src/templates)**: Get products from given URLs

## Products

`qrawl products` fetches pages and extracts products from their schema.org data
(`Product` / `ProductGroup`, including `ItemList`s on category pages), falling
back to Open Graph `product:*` tags. Each product has its name, URL, brand,
price (and `priceMax` for a range), currency, availability, images, rating,
attributes (e.g. `"width": "60 in"`), and variants.

```bash
# Product pages
qrawl products https://shop.example.com/products/round-table https://shop.example.com/products/coupe

# Category / search-results pages that list products (`ItemList`)
qrawl products "https://shop.example.com/search?q=round+dining+table"

# Products from the pages a listing links to: compose with `qrawl children`
# (a listing with no detectable children yields itself)
qrawl children https://shop.example.com/collections/glassware | qrawl products -
```

Every input URL gets an entry: `{"url", "products": [...]}`, or `{"url", "error"}`
when the page couldn't be fetched (blocked, timed out, …).

## License

MIT

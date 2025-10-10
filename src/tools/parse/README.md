# Parse

## Philosophy

**This tool does ONE thing: PARSE.**

Parse detects structure and returns HTML. It doesn't extract data, doesn't classify content, doesn't make decisions - it just identifies patterns in HTML structure and returns clean HTML chunks.

**The crown jewel: parse_siblings()** - Deterministic sibling detection that finds repeating content patterns on any page.

**~640 lines. Zero dependencies. Three functions. Pure parsing.**

## Summary

Parse HTML and return cleaned HTML:

1. **parse_clean()** - Returns clean HTML
2. **parse_main()** - Returns main HTML
3. **parse_siblings()** - Rreturns siblings HTML
4. **parse_children()** - Returns children HTML (ie. siblings with links)

**Returns HTML, not typed data structures. Parse PARSES, extract EXTRACTS.**

## Responsibility

**What parse DOES:**
- ✅ Parse HTML structure and detect patterns
- ✅ Find main content areas
- ✅ Detect siblings using deterministic pattern matching
- ✅ Clean HTML (remove classes, IDs, styles, junk elements)
- ✅ Return HTML strings for downstream tools

**What parse does NOT do:**
- ❌ Extract structured data (use extract tool)
- ❌ Scrape from web (use scrape tool)
- ❌ Classify page type (use classify tool)
- ❌ Map links (use map tool)
- ❌ Return typed data structures (returns HTML)

**Design Principle:** Parse returns HTML. Other tools decide what to do with it.

## API

### Three simple functions:

```rust
parse_clean(html: &str) -> String               // Cleaned HTML
parse_main(html: &str) -> String                // HTML from main content area (tip: combine parse_clean + parse_main to get cleaned HTML from main content area!)
parse_siblings(html: &str) -> Vec<String>       // Sibling HTMLs
parse_children(htmml: $str) -> String           // Children HTML (siblings with links)
```

## parse_siblings() - Sibling Detection Explained ⭐

### The Algorithm

**Deterministic pattern matching - no guessing, no scoring, no heuristics.**

#### 4-Phase Process:

1. **Scan entire DOM tree** at all levels (depth-first traversal)
2. **Find ALL sibling groups** using two pattern types:
   - **Single-element patterns**: Repeated elements sharing core structure
   - **Multi-element patterns**: Sequences like `[A,B,A,B,A,B]` → 3 siblings
3. **Filter trivial elements**: Remove elements without children (br, i, em, etc.)
4. **Select best group** using deterministic 3-tier hierarchy:
   - **Primary**: Inside `<article>` tag? (semantic context)
   - **Secondary**: Most items (quantity)
   - **Tertiary**: Longest pattern (richness)

### Pattern Matching Logic

#### Single-Element Patterns (Common-Prefix Matching)

Elements are siblings if they **share a common core structure** (minimum 2-element prefix):

**Example 1: Exact Match**
```html
<li><div/><div/><div/></li>  → pattern: [div, div, div]
<li><div/><div/><div/></li>  → pattern: [div, div, div]
<li><div/><div/><div/></li>  → pattern: [div, div, div]
```
All 3 match exactly → **3 siblings**

**Example 2: Common-Prefix Match**
```html
<li><div/><div/><div/></li>         → pattern: [div, div, div]
<li><div/><div/><div/></li>         → pattern: [div, div, div]
<li><div/><div/><div/><div/></li>   → pattern: [div, div, div, div]
```
Core pattern `[div, div]` shared by all → **3 siblings** (extra trailing div ignored)

**Why this works:** Items #1-2 have the core content structure. Item #3 has the same core PLUS an extra pagination div. All share the same semantic pattern.

**Elements filtered out:**
- Elements without children: `<br/>`, `<i>text</i>`, `<em>text</em>` (no nested structure = not real siblings)

#### Multi-Element Patterns (Sequence Detection)

Detects repeating sequences of multiple elements:

**Example: 101Cookbooks Cocktails**
```html
<p><strong>1. Cardinale</strong><br/>Blood red...</p>  ← text paragraph
<p><img src="cardinale.jpg"/></p>                      ← image paragraph
<p><strong>2. Blood Orange</strong><br/>I love...</p>  ← text paragraph
<p><img src="blood-orange.jpg"/></p>                   ← image paragraph
<p><strong>3. Jekyll Gin</strong><br/>This twist...</p> ← text paragraph
<p><img src="jekyll.jpg"/></p>                         ← image paragraph
```

**Detected pattern**: `[text-p, img-p]` with pattern_len=2
**Result**: 3 siblings, each containing 2 concatenated `<p>` elements

**How it works:**
1. Scan sequence for repeating multi-element patterns (lengths 2, 3, 4...)
2. Find pattern `[A, B]` that repeats: positions [0,1], [2,3], [4,5]
3. Verify no overlap between instances
4. Group into siblings: `[html(0)+html(1), html(2)+html(3), html(4)+html(5)]`

### Selection Hierarchy (Deterministic)

When multiple sibling groups are found, select using **tuple comparison**:

```rust
max_by_key(|(in_article, pattern_len, group)|
    (*in_article, group.len(), *pattern_len)
)
```

**Comparison order:**
1. **in_article** (bool): `true > false`
   - Content inside `<article>` beats navigation/footer content
2. **group.len()** (usize): Higher count wins
   - 13 items > 10 items > 7 items
3. **pattern_len** (usize): Richer patterns win
   - Multi-element pattern (len=2) > single element (len=1)

**Example Decision Tree:**

```
Groups found:
- A: (true, 13, 1)  ← 13 cocktails in <article>, single-element
- B: (true, 24, 1)  ← 24 nav links in <article>, single-element
- C: (false, 50, 1) ← 50 footer links outside <article>

Comparison:
- A vs B: true==true, then 13<24 → B wins
- B vs C: true>false → B wins

Winner: B (24 nav links)

But wait! After filtering:
- Nav links have pattern [a] (just link tag, no children) → FILTERED OUT
- Cocktails have pattern [div, figure, h2, p] (rich nested content) → KEPT

Final winner: A (13 cocktails)
```

### Real-World Examples

#### Example 1: The Spruce Eats (Common-Prefix Matching)

**URL:** thespruceeats.com/halloween-drinks-5179697

**HTML Structure:**
- 10 cocktails: `<li><div/><div/><div/></li>` (3 child divs)
- 3 cocktails: `<li><div/><div/><div/><div/></li>` (4 child divs - extra pagination)

**Pattern Detection:**
- Core pattern: `[div, div]` shared by all 13
- Extra div in 3 items = optional trailing element
- Common-prefix match groups all 13 together

**Result:** 13 siblings (all cocktails)

#### Example 2: 101 Cookbooks (Multi-Element Pattern)

**URL:** 101cookbooks.com/7-halloween-cocktails

**HTML Structure:**
```html
<article>
  <p><strong>1. Cardinale...</strong></p>
  <p><img src="cardinale.jpg"/></p>
  <p><strong>2. Blood Orange...</strong></p>
  <p><img src="blood-orange.jpg"/></p>
  <!-- ... 5 more pairs ... -->
</article>
```

**Pattern Detection:**
- Sequence of 14 `<p>` elements alternating: text-p, img-p, text-p, img-p...
- Multi-element pattern `[p[strong], p[img]]` detected (pattern_len=2)
- 7 repetitions found

**Result:** 7 siblings, each containing text+image HTML concatenated

#### Example 3: Filtering Trivial Elements

**HTML:**
```html
<article>
  <p>Intro paragraph</p>
  <p><strong>Item 1</strong></p>
  <p><img src="1.jpg"/></p>
  <br/>
  <br/>
  <p><strong>Item 2</strong></p>
  <p><img src="2.jpg"/></p>
</article>
```

**What happens:**
1. Find `<br/>` elements: pattern = `[]` (no children) → **FILTERED OUT**
2. Find `[p, p]` pattern: 2 instances → **2 siblings detected**
3. Selection: (true, 2, 2) wins over any filtered groups

**Result:** 2 siblings (items 1 and 2), ignoring `<br/>` elements

### Output Format

**Each sibling returns:**
- Clean HTML (no classes/IDs/styles)
- Complete structure (all nested elements preserved)
- Semantic tags only (junk removed)
- Ready for downstream tools (map, extract, etc.)

```json
[
  "<li><div>...</div><div><h2><a href=\"...\">Title</a></h2><figure><img src=\"...\"/></figure><p>Description</p></div></li>",
  "<li><div>...</div><div><h2><a href=\"...\">Title 2</a></h2><figure><img src=\"...\"/></figure><p>Description 2</p></div></li>"
]
```

### Use Cases

**Perfect for:**
- Listicles (recipes, products, articles)
- Product grids
- Blog archives
- Photo galleries
- Collection pages
- Any repeating content pattern

**How it works:**

```bash
# Detect siblings in a listicle
parse siblings https://example.com/top-10-recipes

# Returns: 10 HTML chunks, one for each recipe
# Each chunk has: title, link, image, description
# Clean HTML, ready to process further
```

## Usage Examples

### Parse Main Content

```bash
parse main https://example.com/article
# Returns: HTML from <main> or <article> area only
```

### Detect Siblings (Collection Items)

```bash
parse siblings https://www.thespruceeats.com/halloween-cocktails

# Output: Array of sibling HTMLs
[
  { "html": "<li>...</li>" },  // Cocktail 1
  { "html": "<li>...</li>" },  // Cocktail 2
  ...
]
```

### Compose with Other Tools

```bash
# Find links in siblings
parse siblings https://example.com | map -

# Extract data from siblings
parse siblings https://example.com | extract metadata -

# Or use programmatically
let html = fetch("https://...");
let siblings = parse::parse_siblings(&html);
for sibling in siblings {
    let links = map::find_links(&sibling.html);
    // Process each sibling
}
```

## Design Principles

### 1. Single Responsibility
Parse ONLY parses. Returns HTML, not typed data. Every function starts with `parse_`.

### 2. Zero Dependencies
Works on pure HTML strings. No imports from scrape, extract, map, or any other tool.

### 3. Returns HTML
**Not typed data structures.** Returns clean HTML strings that other tools can process however they want.

### 4. Deterministic
No heuristics, no scoring, no guessing. Pattern matching is based on DOM structure (child tag sequences).

### 5. Clean Output
Removes classes, IDs, styles, junk elements. Returns semantic HTML perfect for downstream tools.

## Tool Separation

- **scrape** = scrapes HTML/JSON-LD/metadata from pages
- **classify** = classifies page type
- **parse** = parses HTML, detects patterns, returns HTML (this tool)
- **extract** = extracts structured data from HTML
- **map** = maps link relationships

**Parse finds structure, extract pulls data. Clean separation.**

Each tool does ONE thing. Compose them however you want.

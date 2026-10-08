#!/usr/bin/env bash
# Live product-extraction check against real retailers.
# Per URL: raw curl probe, `qrawl fetch` (HTML saved + structured-data markers),
# then `qrawl products` (product mode) or `qrawl children | qrawl products -` (listing).
set -uo pipefail
Q=${QRAWL:-target/release/qrawl}
OUT=${OUT:-live-out}
LIMIT=${LIMIT:-6}
UA='Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36'
mkdir -p "$OUT"
SUMMARY="$OUT/summary.md"
echo "| site | mode | curl | qrawl fetch | html bytes | ld+json | Product | children | products | errors |" > "$SUMMARY"
echo "|---|---|---|---|---|---|---|---|---|---|" >> "$SUMMARY"

n=0
grep -v '^#' "${SITES:-.github/live/sites.txt}" | while IFS='|' read -r label mode url; do
  label=$(echo "$label" | xargs); mode=$(echo "$mode" | xargs); url=$(echo "$url" | xargs)
  [ -z "$label" ] && continue
  n=$((n+1)); id="$n-$label-$mode"
  echo "::group::$id  $url"

  code=$(curl -sL -A "$UA" -H 'Accept: text/html' -H 'Accept-Language: en-US,en;q=0.9' \
    --compressed --max-time 30 -o /dev/null -w '%{http_code}' "$url" || true)

  if timeout 90 "$Q" fetch "$url" > "$OUT/$id.html" 2> "$OUT/$id.fetch.txt"; then
    fetch="ok $(grep -o 'Profile: .*' "$OUT/$id.fetch.txt" | head -1 | cut -d' ' -f2)"
  else
    fetch="fail: $(grep -oE 'HTTP status [0-9]+|invalid content[^;]*|tunnel|timed out' "$OUT/$id.fetch.txt" | sort | uniq -c | tr -s ' ' | xargs)"
  fi
  bytes=$(wc -c < "$OUT/$id.html")
  ldjson=$(grep -o 'application/ld+json' "$OUT/$id.html" | wc -l)
  product=$(grep -oE '"@type" ?: ?"(Product|ProductGroup)"' "$OUT/$id.html" | wc -l)
  title=$(grep -oiE '<title[^>]*>[^<]{0,90}' "$OUT/$id.html" | head -1 | sed 's/<title[^>]*>//I')
  echo "curl=$code qrawl=$fetch bytes=$bytes ld+json=$ldjson Product=$product title=$title"
  [ -s "$OUT/$id.html" ] && echo "schema types: $("$Q" schemas - < "$OUT/$id.html" 2>/dev/null | jq -c . 2>/dev/null)"

  children="-"
  if [ "$mode" = listing ]; then
    timeout 120 "$Q" children "$url" > "$OUT/$id.children.json" 2>/dev/null
    children=$(jq '.Ok | length' "$OUT/$id.children.json" 2>/dev/null || echo "?")
    echo "children ($children):"; jq -r '.Ok[:6][]' "$OUT/$id.children.json" 2>/dev/null
    # The listing itself (ItemList) plus up to $LIMIT of its children.
    { echo "$url"; jq -r ".Ok[:$LIMIT][]" "$OUT/$id.children.json" 2>/dev/null; } \
      | timeout 300 "$Q" products - > "$OUT/$id.products.json" 2>/dev/null
  else
    timeout 120 "$Q" products "$url" > "$OUT/$id.products.json" 2>/dev/null
  fi
  nprod=$(jq '[.[] | .products // [] | length] | add // 0' "$OUT/$id.products.json" 2>/dev/null || echo "?")
  errs=$(jq '[.[] | select(.error)] | length' "$OUT/$id.products.json" 2>/dev/null || echo "?")
  jq -r '.[] | if .error then "ERROR \(.url): \(.error[:160])" else (.products[] | "  \(.name) | \(.price)-\(.priceMax // "") \(.currency // "") | \(.availability // "-") | imgs=\(.images | length) | variants=\(.variants // [] | length) | attrs=\(.attributes // {} | keys | join(",")) | \(.url)") end' \
    "$OUT/$id.products.json" 2>/dev/null | head -12
  [ "$mode" = product ] && jq -c '.[0].products[0] // empty | .description |= (if . then .[:80] else . end) | .images |= (if . then .[:2] else . end) | .variants |= (if . then .[:2] else . end)' "$OUT/$id.products.json" 2>/dev/null

  echo "| $label | $mode | $code | $fetch | $bytes | $ldjson | $product | $children | $nprod | $errs |" >> "$SUMMARY"
  echo "::endgroup::"
done

echo; cat "$SUMMARY"
[ -n "${GITHUB_STEP_SUMMARY:-}" ] && cat "$SUMMARY" >> "$GITHUB_STEP_SUMMARY"
exit 0

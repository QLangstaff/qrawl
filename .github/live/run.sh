#!/usr/bin/env bash
# Live product-extraction check against real retailers.
# For each listing URL: raw HTTP probe (status / size / structured-data markers),
# qrawl fetch, child discovery, then `qrawl products --children`.
set -uo pipefail
Q=${QRAWL:-target/release/qrawl}
OUT=${OUT:-live-out}
LIMIT=${LIMIT:-6}
UA='Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/129.0.0.0 Safari/537.36'
mkdir -p "$OUT"
SUMMARY="$OUT/summary.md"
echo "| site | http | bytes | ld+json | Product | qrawl fetch | children | pages w/ products | products | errors |" > "$SUMMARY"
echo "|---|---|---|---|---|---|---|---|---|---|" >> "$SUMMARY"

grep -v '^#' "${SITES:-.github/live/sites.txt}" | while IFS='|' read -r label url; do
  label=$(echo "$label" | xargs); url=$(echo "$url" | xargs)
  [ -z "$label" ] && continue
  echo "::group::$label  $url"

  code=$(curl -sL -A "$UA" -H 'Accept: text/html' -H 'Accept-Language: en-US,en;q=0.9' \
    --compressed --max-time 30 -o "$OUT/$label.raw.html" -w '%{http_code}' "$url" || true)
  bytes=$(wc -c < "$OUT/$label.raw.html" 2>/dev/null || echo 0)
  ldjson=$(grep -o 'application/ld+json' "$OUT/$label.raw.html" 2>/dev/null | wc -l)
  product=$(grep -oE '"@type" ?: ?"(Product|ProductGroup)"' "$OUT/$label.raw.html" 2>/dev/null | wc -l)
  echo "curl: $code, $bytes bytes, ld+json=$ldjson, Product=$product"
  head -c 400 "$OUT/$label.raw.html" | tr '\n' ' '; echo

  if timeout 90 "$Q" fetch "$url" > /dev/null 2> "$OUT/$label.fetch.txt"; then
    fetch="ok ($(grep -o 'Profile: .*' "$OUT/$label.fetch.txt" | head -1 | cut -d' ' -f2))"
  else
    fetch="fail"
  fi
  cat "$OUT/$label.fetch.txt"

  timeout 120 "$Q" children "$url" > "$OUT/$label.children.json" 2>/dev/null
  children=$(jq '.Ok | length' "$OUT/$label.children.json" 2>/dev/null || echo "?")
  jq -r '.Ok[:8][]' "$OUT/$label.children.json" 2>/dev/null

  timeout 300 "$Q" products --children --limit "$LIMIT" "$url" > "$OUT/$label.products.json" 2>/dev/null
  pages=$(jq '[.[] | select(.products)] | length' "$OUT/$label.products.json" 2>/dev/null || echo "?")
  nprod=$(jq '[.[] | .products // [] | length] | add // 0' "$OUT/$label.products.json" 2>/dev/null || echo "?")
  errs=$(jq '[.[] | select(.error)] | length' "$OUT/$label.products.json" 2>/dev/null || echo "?")
  jq -r '.[] | if .error then "ERROR \(.url): \(.error[:200])" else (.products[] | "  \(.name) | \(.price) \(.currency // "") | \(.availability // "-") | imgs=\(.images | length) | \(.url)") end' \
    "$OUT/$label.products.json" 2>/dev/null | head -20

  echo "| $label | $code | $bytes | $ldjson | $product | $fetch | $children | $pages | $nprod | $errs |" >> "$SUMMARY"
  echo "::endgroup::"
done

echo; cat "$SUMMARY"
[ -n "${GITHUB_STEP_SUMMARY:-}" ] && cat "$SUMMARY" >> "$GITHUB_STEP_SUMMARY"
exit 0

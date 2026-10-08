#!/usr/bin/env bash
# Look inside fetched pages that load but yield no structured price data:
# which <script> payloads they carry and where price-like fields live.
set -uo pipefail
Q=${QRAWL:-target/release/qrawl}
inspect() {
  local label=$1 url=$2 f="live-out/inspect-$1.html"
  echo "::group::inspect $label $url"
  timeout 90 "$Q" fetch "$url" > "$f" 2>/dev/null
  echo "bytes: $(wc -c < "$f")"
  echo "-- script tags (id/type):"
  grep -oE '<script[^>]*(id|type)="[^"]*"[^>]*>' "$f" | sed -E 's/ (src|nonce|async|defer|crossorigin|data-[a-z-]+)(="[^"]*")?//g' | sort | uniq -c | sort -rn | head -15
  echo "-- itemprop / og price / product meta:"
  grep -oE '(itemprop="(price|priceCurrency|lowPrice|highPrice)"[^>]{0,80}|property="(og|product):price[^"]*"[^>]{0,60})' "$f" | head -6
  echo "-- price-like JSON fields:"
  grep -oE '"(price|salePrice|sellingPrice|regularPrice|lowPrice|highPrice|priceRange|currentPrice|displayPrice|amount)"\s*:\s*("[^"]{0,40}"|[0-9.]+|\{[^}]{0,120})' "$f" | sort | uniq -c | sort -rn | head -12
  echo "-- dollar amounts in text:"
  grep -oE '\$[0-9][0-9,]*(\.[0-9]{2})?' "$f" | sort | uniq -c | sort -rn | head -8
  shift 2
  for marker in "$@"; do echo "-- $marker: $(grep -o "$marker" "$f" | wc -l)"; done
  echo "::endgroup::"
}
mkdir -p live-out
inspect westelm-product https://www.westelm.com/products/jax-round-dining-table-h4705/
inspect anthro-product "https://www.anthropologie.com/shop/morgan-stemless-wine-glasses-set-of-4?color=100"
inspect amazon-product https://amazon.com/LVB-60-Inch-Dining-Kitchen-Industrial/dp/B0F4DLPWQ6 \
  'id="productTitle"' 'class="a-price' 'a-offscreen' 'data-a-dynamic-image' 'id="landingImage"' 'id="productOverview_feature_div"'
inspect amazon-search "https://amazon.com/s?k=60+inch+round+dining+table" \
  'data-component-type="s-search-result"' 'data-asin="B' 'class="a-price' 'class="s-image"'
exit 0

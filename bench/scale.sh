#!/bin/bash
# Scale gates for og (tk-1i9o): 10k / 100k / 500k blocks.
# Measures: build wall time + throughput, index size breakdown, peak RSS,
# warm query latency (hyperfine, 3 warmup + 10 runs), cold first-query latency.
# Reports og-only numbers. Requires: release binary, hyperfine, /usr/bin/time.
set -u
cd "$(dirname "$0")/.."

OG="$(pwd)/target/release/og"
ROOT=/tmp/bench-scale
rm -rf "$ROOT"; mkdir -p "$ROOT"

# files-per-scale calibrated at ~8 blocks/file
SCALES="${1:-1250:10k 12500:100k 62500:500k}"

QUERIES=(
  "rebalance_global_42"
  "estimate_cost"
  "validate_key quota"
  "how to estimate request cost"
  "flush buffered entries"
  "ConfigEntry retries"
  "resolve_quota"
  "migrate ledger state"
)

echo "# og scale gates — $(date -u +%F)"
echo "# $(sysctl -n machdep.cpu.brand_string), $(sysctl -n hw.ncpu) cores, $(rustc --version)"
echo

for spec in $SCALES; do
  files="${spec%%:*}"; label="${spec##*:}"
  dir="$ROOT/$label"
  python3 bench/gen_corpus.py "$dir" "$files" >/dev/null
  echo "## scale $label ($files files)"

  echo "### build (potion default)"
  /usr/bin/time -l "$OG" build -q "$dir" 2>&1 | grep -E "maximum resident|real" | sed 's/^/  /'
  (cd "$dir" && $OG status 2>/dev/null | grep -E "Blocks|Files")
  echo "  sizes:"; (cd "$dir" && GEN=$(cat .og/CURRENT) && du -h ".og/generations/$GEN/catalog.sqlite" ".og/generations/$GEN/vectors-000.bin" | sed 's/^/    /')

  echo "### warm query latency (hyperfine, 3 warmup + 10 runs)"
  for q in "${QUERIES[@]}"; do
    hyperfine --warmup 3 --runs 10 --style basic -n "$q" "$OG '$q' -n 10 -q '$dir' >/dev/null" 2>&1 \
      | grep -E "Time \(mean|Range" | sed "s/^/  [$q] /"
  done

  echo "### fresh-process query (warmup 0, page cache warm from build) + search RSS"
  (cd "$dir" && sync && hyperfine --warmup 0 --runs 3 --style basic -n cold "$OG 'estimate_cost' -n 10 -q . >/dev/null" 2>&1 | grep "Time (mean" | sed 's/^/  /')
  /usr/bin/time -l "$OG" 'estimate_cost' -n 10 -q "$dir" >/dev/null 2>"$ROOT/rss.txt"; grep "maximum resident" "$ROOT/rss.txt" | sed 's/^/  search /'
  echo
done

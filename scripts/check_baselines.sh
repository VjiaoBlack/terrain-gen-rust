#!/usr/bin/env bash
# Run golden seeds and compare against baseline expectations.
# Exit code 0 = all pass, 1 = at least one regression.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

BASELINE_DIR="tests/baselines"
PASS=0
FAIL=0

if ! command -v jq &>/dev/null; then
  echo "ERROR: jq required (brew install jq)"
  exit 1
fi

cargo build --release 2>/dev/null

for baseline in "$BASELINE_DIR"/seed_*.json; do
  seed=$(jq -r '.seed' "$baseline")
  tick=$(jq -r '.tick' "$baseline")

  echo "Running seed $seed for $tick ticks..."
  actual=$(cargo run --release -- \
    --screenshot --seed "$seed" --width 80 --height 30 \
    --ticks "$tick" --auto-build --diagnostics 2>/dev/null)

  # Check survival
  pop=$(echo "$actual" | jq '.population')
  expected_survived=$(jq -r '.expected.survived' "$baseline")
  if [ "$expected_survived" = "true" ] && [ "$pop" = "0" ]; then
    echo "  FAIL: seed $seed died (expected survival)"
    FAIL=$((FAIL + 1))
    continue
  fi

  # Check numeric ranges
  SEED_OK=true
  for field in population; do
    val=$(echo "$actual" | jq ".$field")
    min=$(jq ".expected.${field}.min // empty" "$baseline" 2>/dev/null)
    max=$(jq ".expected.${field}.max // empty" "$baseline" 2>/dev/null)
    tolerance=$(jq '.tolerance' "$baseline")

    if [ -n "$min" ] && [ -n "$max" ]; then
      adj_min=$(echo "$min * (1 - $tolerance)" | bc -l)
      adj_max=$(echo "$max * (1 + $tolerance)" | bc -l)
      outside=$(echo "$val < $adj_min || $val > $adj_max" | bc -l 2>/dev/null || echo "0")
      if [ "$outside" = "1" ]; then
        echo "  WARN: seed $seed $field=$val outside [$(printf '%.0f' "$adj_min"), $(printf '%.0f' "$adj_max")]"
        SEED_OK=false
      fi
    fi
  done

  # Check terrain metrics
  water=$(echo "$actual" | jq '.terrain.water_coverage_pct')
  water_min=$(jq '.expected.water_coverage_pct.min // empty' "$baseline" 2>/dev/null)
  water_max=$(jq '.expected.water_coverage_pct.max // empty' "$baseline" 2>/dev/null)
  if [ -n "$water_min" ] && [ -n "$water_max" ]; then
    outside=$(echo "$water < $water_min || $water > $water_max" | bc -l 2>/dev/null || echo "0")
    if [ "$outside" = "1" ]; then
      echo "  WARN: seed $seed water_coverage=${water}% outside [${water_min}, ${water_max}]"
      SEED_OK=false
    fi
  fi

  # Check biome count
  biomes=$(echo "$actual" | jq '.terrain.biome_distribution | length')
  biome_min=$(jq '.expected.biome_types.min // empty' "$baseline" 2>/dev/null)
  if [ -n "$biome_min" ]; then
    if [ "$biomes" -lt "$biome_min" ]; then
      echo "  WARN: seed $seed biome_types=$biomes < min $biome_min"
      SEED_OK=false
    fi
  fi

  if [ "$SEED_OK" = true ]; then
    echo "  PASS: seed $seed (pop=$pop, water=${water}%, biomes=$biomes)"
    PASS=$((PASS + 1))
  else
    FAIL=$((FAIL + 1))
  fi
done

echo ""
echo "Results: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]

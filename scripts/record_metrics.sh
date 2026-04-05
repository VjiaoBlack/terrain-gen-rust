#!/usr/bin/env bash
# Append current metrics to docs/metrics_history.json
# Usage: ./scripts/record_metrics.sh
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

if ! command -v jq &>/dev/null; then
  echo "ERROR: jq required (brew install jq)"
  exit 1
fi

COMMIT=$(git rev-parse --short HEAD)
DATE=$(date +%Y-%m-%d)
HISTORY="docs/metrics_history.json"

# Initialize if missing
if [ ! -f "$HISTORY" ]; then
  echo "[]" > "$HISTORY"
fi

cargo build --release 2>/dev/null

echo "Recording metrics for commit $COMMIT..."

# Build seeds JSON object
SEEDS_JSON="{"
for seed in 42 137 777; do
  metrics=$(cargo run --release -- \
    --screenshot --seed "$seed" --width 80 --height 30 \
    --ticks 12000 --auto-build --diagnostics 2>/dev/null)

  pop=$(echo "$metrics" | jq '.population')
  food=$(echo "$metrics" | jq '.resources.food')
  buildings=$(echo "$metrics" | jq '[.buildings | to_entries[] | .value] | add // 0')
  survived=$(echo "$metrics" | jq '.population > 0')
  water=$(echo "$metrics" | jq '.terrain.water_coverage_pct')

  SEEDS_JSON="${SEEDS_JSON}\"${seed}\": {\"population\": $pop, \"food\": $food, \"buildings\": $buildings, \"survived\": $survived, \"water_pct\": $water},"
done
SEEDS_JSON="${SEEDS_JSON%,}}"

# Build entry and append
ENTRY="{\"date\": \"$DATE\", \"commit\": \"$COMMIT\", \"seeds\": $SEEDS_JSON}"
jq ". + [$ENTRY]" "$HISTORY" > "${HISTORY}.tmp"
mv "${HISTORY}.tmp" "$HISTORY"

echo "Recorded metrics for $DATE ($COMMIT)"
echo "$ENTRY" | jq .

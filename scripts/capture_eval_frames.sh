#!/usr/bin/env bash
# Capture evaluation frames: PNG screenshots + JSON state dumps + report cards
# for 3 golden seeds at 4 timepoints each.
# Usage: ./scripts/capture_eval_frames.sh [output_dir]
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

OUTDIR="${1:-eval_frames}"
mkdir -p "$OUTDIR"

echo "Building with PNG feature..."
cargo build --release --features png 2>/dev/null

SEEDS="42 137 777"
TICKS="100 1000 5000 10000"

for seed in $SEEDS; do
  for tick in $TICKS; do
    name="seed${seed}_tick${tick}"
    echo "  Capturing $name..."

    # PNG screenshot
    cargo run --release --features png -- \
      --screenshot --seed "$seed" --width 160 --height 48 \
      --ticks "$tick" --auto-build \
      --png "${OUTDIR}/${name}.png" 2>/dev/null

    # JSON state dump
    cargo run --release --features png -- \
      --screenshot --seed "$seed" --width 80 --height 30 \
      --ticks "$tick" --auto-build --diagnostics \
      > "${OUTDIR}/${name}.json" 2>/dev/null

    # Report card
    cargo run --release --features png -- \
      --screenshot --seed "$seed" --width 80 --height 30 \
      --ticks "$tick" --auto-build --report-card \
      > "${OUTDIR}/${name}_report.json" 2>/dev/null
  done
done

echo ""
PNG_COUNT=$(ls "${OUTDIR}"/*.png 2>/dev/null | wc -l | tr -d ' ')
JSON_COUNT=$(ls "${OUTDIR}"/*.json 2>/dev/null | wc -l | tr -d ' ')
echo "Done! Captured ${PNG_COUNT} PNGs + ${JSON_COUNT} JSON files in ${OUTDIR}/"

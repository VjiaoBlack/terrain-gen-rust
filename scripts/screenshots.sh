#!/bin/bash
# Generate PNG screenshots of different game states
# Usage: ./scripts/screenshots.sh
# Requires: cargo with --features png
set -e
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."

OUTDIR="tapes/screenshots"
mkdir -p "$OUTDIR"
rm -f "$OUTDIR"/*.png

# Build once with PNG support
echo "Building with PNG feature..."
cargo build --release --features png 2>/dev/null

take_screenshot() {
    local name=$1
    local ticks=$2
    local seed=$3
    local width=${4:-120}
    local height=${5:-40}

    echo "  $name (seed=$seed ticks=$ticks ${width}x${height})"
    cargo run --release --features png -- \
        --screenshot --width "$width" --height "$height" \
        --ticks "$ticks" --seed "$seed" \
        --png "${OUTDIR}/${name}.png" 2>/dev/null
}

echo "Generating screenshots..."

# Fresh start
take_screenshot "01_spring_start" 100 42

# Village growing
take_screenshot "02_village_growing" 2000 42

# Winter scene
take_screenshot "03_winter_night" 4500 42

# Summer day
take_screenshot "04_summer_day" 3000 42

# Different landscape — coastal
take_screenshot "05_coastal" 200 137

# Different landscape — mountainous
take_screenshot "06_mountains" 200 999

# Late game established
take_screenshot "07_established" 8000 42

# Wide panoramic view
take_screenshot "08_wide_view" 1000 42 160 50

echo ""
echo "Done! Screenshots:"
ls -lh "$OUTDIR"/*.png
echo ""
echo "View: open $OUTDIR/"

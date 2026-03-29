#!/bin/bash
# Generate PNG screenshots of different game states
# Usage: ./scripts/screenshots.sh
set -e
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."

OUTDIR="tapes/screenshots"
mkdir -p "$OUTDIR"
rm -f "$OUTDIR"/*.png

echo "Building with PNG feature..."
cargo build --release --features png 2>/dev/null

# Season timing: 1200 ticks/day, 12000 ticks/season
# Spring=0, Summer=12000, Autumn=24000, Winter=36000
# Midday ~+600 ticks, Night ~+1000 ticks

take() {
    local name=$1 ticks=$2 seed=$3 w=${4:-120} h=${5:-40}
    echo "  $name"
    cargo run --release --features png -- \
        --screenshot --width "$w" --height "$h" \
        --ticks "$ticks" --seed "$seed" \
        --png "${OUTDIR}/${name}.png" 2>/dev/null
}

echo "Generating screenshots..."

# Seasons at midday (+600 ticks into each season)
take "01_spring_day"     600    42
take "02_summer_day"     12600  42
take "03_autumn_day"     24600  42
take "04_winter_day"     36600  42

# Night scenes
take "05_summer_night"   13000  42
take "06_winter_night"   37000  42

# Different landscapes
take "07_coastal"        600   137
take "08_mountains"      600   999
take "09_islands"        600   7

# Late game, wide view
take "10_established"    25000  42
take "11_wide_view"      13000  42 160 50

echo ""
echo "Done!"
ls -lh "$OUTDIR"/*.png
echo "View: open $OUTDIR/"

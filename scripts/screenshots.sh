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

# Game starts at hour 10. tick_rate=0.02, so 1 hour = 50 ticks.
# Season starts: Spring=0, Summer=12000, Autumn=24000, Winter=36000
# Noon (hour 12) = +100 ticks from season start
# Midnight = +700, 1am (peak moon) = +750

take() {
    local name=$1 ticks=$2 seed=$3 w=${4:-160} h=${5:-48} extra=${6:-}
    echo "  $name"
    cargo run --release --features png -- \
        --screenshot --width "$w" --height "$h" \
        --ticks "$ticks" --seed "$seed" $extra \
        --png "${OUTDIR}/${name}.png" 2>&1 | grep "=== State"
}

echo "Generating screenshots..."

# Seasons at noon
take "01_spring_noon"      100    42
take "02_summer_noon"      12100  42
take "03_autumn_noon"      24100  42
take "04_winter_noon"      36100  42

# Moonlit nights (1am, peak moon)
take "05_summer_moonlit"   12750  42
take "06_winter_moonlit"   36750  42

# Different terrains
take "07_coastal"          100   88
take "08_forest_heavy"     100   50

# Established settlement — auto-build, stop in autumn before winter kills
take "09_established"      25000  42  160 48 "--auto-build"

# Wide panoramic
take "10_panoramic"        12100  42  200 55

echo ""
echo "Done!"
ls -lh "$OUTDIR"/*.png
echo "View: open $OUTDIR/"

#!/bin/bash
# Generate PNG screenshots of different game states
# Usage: ./scripts/screenshots.sh
# Requires: vhs, cargo
set -e
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."

OUTDIR="tapes/screenshots"
mkdir -p "$OUTDIR"

# Helper: run --screenshot with params, capture ANSI, convert last VHS frame to PNG
take_screenshot() {
    local name=$1
    local ticks=$2
    local seed=$3
    local width=${4:-120}
    local height=${5:-40}
    local extra_inputs=$6

    echo "Capturing: $name (seed=$seed ticks=$ticks ${width}x${height})..."

    # Create a minimal tape that just shows the screenshot output
    local tape=$(mktemp /tmp/vhs_XXXXXX.tape)
    cat > "$tape" <<EOF
Output ${OUTDIR}/${name}.gif
Set FontSize 13
Set Width 1100
Set Height 600
Set Theme "Catppuccin Mocha"
Set Shell "zsh"
Set TypingSpeed 0

Hide
Type "export PATH=\$HOME/.cargo/bin:\$PATH && cd ~/Projects/terrain-gen-rust"
Enter
Sleep 1s
Type "cargo run --release -- --screenshot --width ${width} --height ${height} --ticks ${ticks} --seed ${seed}"
Enter
Sleep 8s
Show
Sleep 1s
EOF

    vhs "$tape" 2>/dev/null
    rm -f "$tape"
}

# 1. Fresh start — spring morning
take_screenshot "01_spring_start" 60 42

# 2. Settled village — after some building
take_screenshot "02_village_growing" 2000 42

# 3. Winter night — dark, cold
take_screenshot "03_winter_night" 4500 42

# 4. Summer day — bright
take_screenshot "04_summer_day" 3000 42

# 5. Different seed — coastal/varied terrain
take_screenshot "05_coastal_terrain" 200 137

# 6. Different seed — mountainous
take_screenshot "06_mountain_start" 200 999

# 7. Large established settlement
take_screenshot "07_established" 8000 42

# 8. Zoomed out — wider view
take_screenshot "08_wide_view" 1000 42 160 50

echo ""
echo "Screenshots saved to $OUTDIR/"
ls -lh "$OUTDIR"/*.gif

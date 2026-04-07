#!/usr/bin/env bash
# check_baselines.sh — Verify key simulation metrics haven't regressed.
# Runs 3 seeds to tick 12000 and checks population/water baselines.
# Must exit 0 on current (passing) code. Slower than validate_features.sh (~3 min).
set -euo pipefail

FAILURES=0
CHECKS=0

check() {
    local desc="$1"
    local result="$2"
    CHECKS=$((CHECKS + 1))
    if [ "$result" = "ok" ]; then
        echo "  PASS: $desc"
    else
        echo "  FAIL: $desc — $result"
        FAILURES=$((FAILURES + 1))
    fi
}

check_gte() {
    local desc="$1"
    local val="$2"
    local min="$3"
    check "$desc >= $min" "$(awk "BEGIN{exit !($val >= $min)}" && echo ok || echo "$val < $min")"
}

check_between() {
    local desc="$1"
    local val="$2"
    local lo="$3"
    local hi="$4"
    check "$desc in [$lo, $hi]" "$(awk "BEGIN{exit !($val >= $lo && $val <= $hi)}" && echo ok || echo "$val out of range [$lo, $hi]")"
}

echo "=== check_baselines.sh — 3 seeds × 12K ticks ==="
echo "(This takes ~3 minutes)"
echo ""

for seed in 42 137 777; do
    echo "--- Seed $seed ---"
    DIAG=$(cargo run --release -- --play --seed "$seed" --width 40 --height 15 \
        --ticks 12000 --auto-build --diagnostics 2>/dev/null | tail -1)

    POP=$(echo "$DIAG" | grep -oP '"population":\K\d+' || echo "0")
    WATER=$(echo "$DIAG" | grep -oP '"water_coverage_pct":\K[0-9.]+' || echo "0")
    PIPE=$(echo "$DIAG" | grep -oP '"pipe_water_total":\K[0-9.]+' || echo "0")
    WIND=$(echo "$DIAG" | grep -oP '"wind_moisture_total":\K[0-9.]+' || echo "0")
    VEG=$(echo "$DIAG" | grep -oP '"avg_vegetation":\K[0-9.]+' || echo "0")
    FOOD=$(echo "$DIAG" | grep -oP '"food":\K\d+' || echo "0")
    TICK=$(echo "$DIAG" | grep -oP '"tick":\K\d+' || echo "0")

    check "seed $seed: reached tick 12000" \
        "$([ "$TICK" -ge 12000 ] && echo ok || echo "only reached tick $TICK")"
    check_gte "seed $seed: population survived (>= 1)" "$POP" 1
    check_gte "seed $seed: food > 0 at tick 12000" "$FOOD" 1
    check_between "seed $seed: water coverage in [5, 60]%" "$WATER" 5 60
    check_gte "seed $seed: pipe_water_total > 100" "$PIPE" 100
    check_gte "seed $seed: wind_moisture_total > 0" "$WIND" 1
    check_gte "seed $seed: avg_vegetation > 0" "$VEG" 0.01
    echo ""
done

# Cross-seed diversity check: seeds 42 and 137 should have different water coverage
WATER_42=$(cargo run --release -- --play --seed 42 --width 40 --height 15 \
    --ticks 100 --auto-build --diagnostics 2>/dev/null | tail -1 | \
    grep -oP '"water_coverage_pct":\K[0-9.]+' || echo "0")
WATER_137=$(cargo run --release -- --play --seed 137 --width 40 --height 15 \
    --ticks 100 --auto-build --diagnostics 2>/dev/null | tail -1 | \
    grep -oP '"water_coverage_pct":\K[0-9.]+' || echo "0")
DIFF=$(awk "BEGIN{d=$WATER_137-$WATER_42; print (d<0)?-d:d}")
check "seeds 42 vs 137 have different water coverage (diff >= 5%)" \
    "$(awk "BEGIN{exit !($DIFF >= 5)}" && echo ok || echo "diff=$DIFF% (seeds too similar)")"

echo ""
echo "=== Results: $((CHECKS - FAILURES))/$CHECKS passed ==="
if [ "$FAILURES" -gt 0 ]; then
    echo "BASELINE CHECK FAILED: $FAILURES regression(s) detected"
    exit 1
else
    echo "All baselines passed."
    exit 0
fi

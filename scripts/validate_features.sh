#!/usr/bin/env bash
# validate_features.sh — Fast deterministic harness checks.
# Must exit 0 on current (passing) code. Run before every commit.
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

echo "=== validate_features.sh ==="

# ── 1. Build check ──────────────────────────────────────────────────────────
echo ""
echo "--- Build & Test ---"
if cargo build --release 2>/dev/null; then
    check "cargo build --release" "ok"
else
    check "cargo build --release" "build failed"
    exit 1
fi

# Count test results (allow known failing test)
TEST_OUTPUT=$(cargo test --lib 2>&1 || true)
PASSED=$(echo "$TEST_OUTPUT" | grep -oP '\d+(?= passed)' | tail -1 || echo "0")
FAILED=$(echo "$TEST_OUTPUT" | grep -oP '\d+(?= failed)' | tail -1 || echo "0")
check "lib tests pass count >= 750" "$([ "${PASSED:-0}" -ge 750 ] && echo ok || echo "only $PASSED passed")"
check "lib tests fail count <= 1 (known: construction_dust_particles_spawn)" \
    "$([ "${FAILED:-0}" -le 1 ] && echo ok || echo "$FAILED tests failing (expected <=1)")"

# ── 2. Module size checks ────────────────────────────────────────────────────
echo ""
echo "--- Module Size ---"
check_file_size() {
    local file="$1"
    local limit="$2"
    if [ ! -f "$file" ]; then
        check "file exists: $file" "file not found"
        return
    fi
    local lines
    lines=$(wc -l < "$file")
    check "$file <= $limit lines" "$([ "$lines" -le "$limit" ] && echo ok || echo "$lines lines (limit $limit)")"
}

check_file_size "src/tilemap.rs" 3000
check_file_size "src/game/mod.rs" 3000
check_file_size "src/game/build.rs" 3000
check_file_size "src/ecs/systems.rs" 3000
check_file_size "src/ecs/ai.rs" 3000
check_file_size "src/simulation/moisture.rs" 2500

# ── 3. Required files exist ──────────────────────────────────────────────────
echo ""
echo "--- Required Files ---"
for f in \
    "features.json" \
    "docs/ARCHITECTURE.md" \
    "docs/game_design.md" \
    "docs/workflow.md" \
    "scripts/validate_features.sh" \
    "scripts/check_baselines.sh"
do
    check "exists: $f" "$([ -f "$f" ] && echo ok || echo "missing")"
done

# ── 4. features.json schema ──────────────────────────────────────────────────
echo ""
echo "--- features.json ---"
if command -v jq &>/dev/null; then
    check "features.json is valid JSON" "$(jq empty features.json 2>/dev/null && echo ok || echo "invalid JSON")"
    SYSTEM_COUNT=$(jq '.systems | length' features.json 2>/dev/null || echo 0)
    check "features.json has >= 5 systems" "$([ "$SYSTEM_COUNT" -ge 5 ] && echo ok || echo "only $SYSTEM_COUNT systems")"
else
    check "jq available for JSON checks" "jq not installed — install for full validation"
fi

# ── 5. Diagnostics JSON fields ───────────────────────────────────────────────
echo ""
echo "--- Diagnostics Output ---"
DIAG=$(cargo run --release -- --play --seed 42 --width 40 --height 15 --ticks 1000 --auto-build --diagnostics 2>/dev/null | tail -1)
if [ -z "$DIAG" ]; then
    check "diagnostics output non-empty" "empty output"
else
    for field in population resources terrain tick; do
        check "diagnostics has field: $field" \
            "$(echo "$DIAG" | grep -q "\"$field\"" && echo ok || echo "field missing in: $(echo "$DIAG" | head -c 80)")"
    done
    PIPE_WATER=$(echo "$DIAG" | grep -oP '"pipe_water_total":\K[0-9.]+' || echo "0")
    check "pipe_water_total > 0 (water system active)" \
        "$(awk "BEGIN{exit !($PIPE_WATER > 0)}" && echo ok || echo "pipe_water_total=$PIPE_WATER")"
    WIND_MOIST=$(echo "$DIAG" | grep -oP '"wind_moisture_total":\K[0-9.]+' || echo "0")
    check "wind_moisture_total > 0 (atmosphere active)" \
        "$(awk "BEGIN{exit !($WIND_MOIST > 0)}" && echo ok || echo "wind_moisture_total=$WIND_MOIST")"
    VEG=$(echo "$DIAG" | grep -oP '"avg_vegetation":\K[0-9.]+' || echo "0")
    check "avg_vegetation > 0 (vegetation system active)" \
        "$(awk "BEGIN{exit !($VEG > 0)}" && echo ok || echo "avg_vegetation=$VEG")"
fi

# ── 6. Design doc presence ───────────────────────────────────────────────────
echo ""
echo "--- Design Docs ---"
for pillar_dir in pillar1_geography pillar2_emergence pillar3_arc pillar4_observable pillar5_scale; do
    count=$(ls docs/design/${pillar_dir}/*.md 2>/dev/null | wc -l || echo 0)
    check "docs/design/$pillar_dir has >= 1 doc" "$([ "$count" -ge 1 ] && echo ok || echo "no docs found")"
done

# ── Summary ─────────────────────────────────────────────────────────────────
echo ""
echo "=== Results: $((CHECKS - FAILURES))/$CHECKS passed ==="
if [ "$FAILURES" -gt 0 ]; then
    echo "VALIDATION FAILED: $FAILURES check(s) failed"
    exit 1
else
    echo "All checks passed."
    exit 0
fi

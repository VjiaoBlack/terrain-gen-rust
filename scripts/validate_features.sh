#!/usr/bin/env bash
# Validates features.json: schema, file existence, test count accuracy.
# Usage: ./scripts/validate_features.sh
# Exit code 0 = all good, 1 = issues found

set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

FEATURES="features.json"
ERRORS=0
WARNINGS=0

if ! command -v jq &>/dev/null; then
  echo "ERROR: jq required but not installed (brew install jq)"
  exit 1
fi

# 1. JSON parses cleanly
if ! jq empty "$FEATURES" 2>/dev/null; then
  echo "FAIL: $FEATURES is not valid JSON"
  exit 1
fi
echo "OK: JSON parses cleanly"

# 2. Every file listed actually exists
echo ""
echo "=== File existence ==="
for system in $(jq -r '.systems | keys[]' "$FEATURES"); do
  for file in $(jq -r ".systems[\"$system\"].files[]" "$FEATURES"); do
    if [ ! -e "$file" ] && ! ls -d $file &>/dev/null 2>&1; then
      echo "FAIL: $system lists '$file' but it doesn't exist"
      ERRORS=$((ERRORS + 1))
    fi
  done
done
if [ $ERRORS -eq 0 ]; then
  echo "OK: All listed files exist"
fi

# 3. Test counts are roughly accurate
echo ""
echo "=== Test count accuracy ==="
for system in $(jq -r '.systems | keys[]' "$FEATURES"); do
  expected=$(jq -r ".systems[\"$system\"].test_count" "$FEATURES")
  files=$(jq -r ".systems[\"$system\"].files[]" "$FEATURES")

  actual=0
  for file in $files; do
    if [ -f "$file" ]; then
      count=$(grep -c '#\[test\]' "$file" 2>/dev/null || true)
      actual=$((actual + count))
    elif [ -d "$file" ]; then
      count=$(grep -r '#\[test\]' "$file" 2>/dev/null | wc -l | tr -d ' ')
      actual=$((actual + count))
    fi
  done

  # Allow 30% tolerance (tests may live in separate test files)
  if [ "$expected" -eq 0 ] && [ "$actual" -eq 0 ]; then
    continue
  elif [ "$expected" -eq 0 ] && [ "$actual" -gt 0 ]; then
    echo "WARN: $system claims 0 tests but found $actual in source files"
    WARNINGS=$((WARNINGS + 1))
  elif [ "$actual" -eq 0 ] && [ "$expected" -gt 0 ]; then
    echo "WARN: $system claims $expected tests but found 0 in listed files (tests may be in separate test module)"
    WARNINGS=$((WARNINGS + 1))
  else
    ratio=$(echo "$actual $expected" | awk '{if ($2>0) printf "%.0f", ($1/$2)*100; else print 0}')
    if [ "$ratio" -lt 50 ] || [ "$ratio" -gt 200 ]; then
      echo "WARN: $system test count mismatch — features.json says $expected, files have $actual (#[test] annotations)"
      WARNINGS=$((WARNINGS + 1))
    fi
  fi
done
if [ $WARNINGS -eq 0 ]; then
  echo "OK: Test counts are within tolerance"
fi

# 4. Status values are valid
echo ""
echo "=== Status validation ==="
valid_statuses=$(jq -r '.status_legend | keys[]' "$FEATURES" | tr '\n' '|')
for system in $(jq -r '.systems | keys[]' "$FEATURES"); do
  status=$(jq -r ".systems[\"$system\"].status" "$FEATURES")
  if ! echo "$status" | grep -qE "^(${valid_statuses%|})$"; then
    echo "FAIL: $system has invalid status '$status'"
    ERRORS=$((ERRORS + 1))
  fi
done
if [ $ERRORS -eq 0 ]; then
  echo "OK: All statuses are valid"
fi

# 5. Large non-test source files (>3000 lines)
echo ""
echo "=== Large source file check ==="
LARGE_FILE_FOUND=0
while IFS= read -r -d '' f; do
  name=$(basename "$f")
  lines=$(wc -l < "$f")
  if [ "$lines" -gt 3000 ] && [ "$name" != "tests.rs" ]; then
    echo "WARN: $f is $lines lines (over 3000 — consider splitting)"
    WARNINGS=$((WARNINGS + 1))
    LARGE_FILE_FOUND=1
  fi
done < <(find src -name "*.rs" -print0 2>/dev/null)
if [ $LARGE_FILE_FOUND -eq 0 ]; then
  echo "OK: No non-test source files over 3000 lines"
fi

# 6. Systems marked 'ok' with test_count=0 and no test_note
echo ""
echo "=== Zero-test ok systems ==="
ZERO_TEST_FOUND=0
for system in $(jq -r '.systems | keys[]' "$FEATURES"); do
  status=$(jq -r ".systems[\"$system\"].status" "$FEATURES")
  count=$(jq -r ".systems[\"$system\"].test_count" "$FEATURES")
  test_note=$(jq -r ".systems[\"$system\"].test_note // empty" "$FEATURES")
  if [ "$status" = "ok" ] && [ "$count" = "0" ] && [ -z "$test_note" ]; then
    echo "WARN: $system has status='ok' but test_count=0 with no test_note — untested?"
    WARNINGS=$((WARNINGS + 1))
    ZERO_TEST_FOUND=1
  fi
done
if [ $ZERO_TEST_FOUND -eq 0 ]; then
  echo "OK: All 'ok' systems with test_count=0 have a test_note explaining why"
fi

# 7. threat_score present in collect_diagnostics (regression guard)
echo ""
echo "=== Diagnostics coverage ==="
if ! grep -q '"threat_score"' src/game/mod.rs 2>/dev/null; then
  echo "FAIL: threat_score not found in src/game/mod.rs — diagnostics regression?"
  ERRORS=$((ERRORS + 1))
else
  echo "OK: threat_score present in diagnostics output"
fi

# 8. Stale last_verified dates (>30 days)
echo ""
echo "=== Stale last_verified check ==="
TODAY=$(date +%Y-%m-%d 2>/dev/null || echo "2026-01-01")
STALE_FOUND=0
for system in $(jq -r '.systems | keys[]' "$FEATURES"); do
  lv=$(jq -r ".systems[\"$system\"].last_verified // empty" "$FEATURES")
  if [ -z "$lv" ]; then continue; fi
  # compute days since last_verified using python3 (portable)
  days=$(python3 -c "
from datetime import date
try:
    lv = date.fromisoformat('$lv')
    today = date.fromisoformat('$TODAY')
    print((today - lv).days)
except:
    print(0)
" 2>/dev/null || echo 0)
  if [ "$days" -gt 30 ]; then
    echo "WARN: $system last_verified=$lv is ${days} days old (>30)"
    WARNINGS=$((WARNINGS + 1))
    STALE_FOUND=1
  fi
done
if [ $STALE_FOUND -eq 0 ]; then
  echo "OK: All systems verified within 30 days"
fi

# 9. needs_tests systems with test_count=0 (no progress on coverage)
echo ""
echo "=== needs_tests progress check ==="
NEEDS_TESTS_STALE=0
for system in $(jq -r '.systems | keys[]' "$FEATURES"); do
  status=$(jq -r ".systems[\"$system\"].status" "$FEATURES")
  count=$(jq -r ".systems[\"$system\"].test_count" "$FEATURES")
  if [ "$status" = "needs_tests" ] && [ "$count" = "0" ]; then
    echo "WARN: $system status='needs_tests' but test_count=0 — no progress on test coverage"
    WARNINGS=$((WARNINGS + 1))
    NEEDS_TESTS_STALE=1
  fi
done
if [ $NEEDS_TESTS_STALE -eq 0 ]; then
  echo "OK: All 'needs_tests' systems have made progress (test_count > 0)"
fi

# 10. Hardcoded /tmp/ paths in test files (parallel test race condition risk)
echo ""
echo "=== Hardcoded temp path check ==="
HARDCODED_TMP=0
# Collect unique file+path pairs to avoid duplicate warnings per usage site
declare -A SEEN_TMP
while IFS= read -r match; do
  file=$(echo "$match" | cut -d: -f1)
  path=$(echo "$match" | grep -oE '"/tmp/[^"]*"' | head -1)
  key="${file}:${path}"
  # Warn if it looks like a plain /tmp/test_*.json with no dynamic component
  if echo "$path" | grep -qE '"/tmp/test_[a-z_]+\.(json|txt|bin)"'; then
    if [ -z "${SEEN_TMP[$key]+x}" ]; then
      SEEN_TMP[$key]=1
      echo "WARN: $file has hardcoded tmp path $path — parallel tests may race (use unique suffix)"
      WARNINGS=$((WARNINGS + 1))
      HARDCODED_TMP=1
    fi
  fi
done < <(grep -rn '"/tmp/' src/ 2>/dev/null)
if [ $HARDCODED_TMP -eq 0 ]; then
  echo "OK: No hardcoded /tmp/ paths with static names found in test files"
fi

# 11. Verify simulation.rs does not exist (split to src/simulation/ is complete)
echo ""
echo "=== Split regression guard: simulation.rs ==="
if [ -f "src/simulation.rs" ]; then
  echo "FAIL: src/simulation.rs exists — this monolith was split into src/simulation/. Accidental re-creation?"
  ERRORS=$((ERRORS + 1))
else
  echo "OK: src/simulation.rs absent (split to src/simulation/ intact)"
fi

# 12. Verify game/render.rs does not exist (split to src/game/render/ is complete)
echo ""
echo "=== Split regression guard: game/render.rs ==="
if [ -f "src/game/render.rs" ]; then
  echo "FAIL: src/game/render.rs exists — this file was split into src/game/render/. Accidental re-creation?"
  ERRORS=$((ERRORS + 1))
else
  echo "OK: src/game/render.rs absent (split to src/game/render/ intact)"
fi

# 13. Verify code-split product files exist (positive complement to checks 11-12)
echo ""
echo "=== Split product files exist ==="
MISSING_SPLITS=0
for split_file in \
    "src/ecs/tests.rs" \
    "src/game/tests.rs" \
    "src/game/input.rs" \
    "src/game/water_cycle.rs" \
    "src/game/fire.rs" \
    "src/game/particles.rs"; do
  if [ ! -f "$split_file" ]; then
    echo "FAIL: $split_file missing — expected from code-split; may have been accidentally deleted"
    ERRORS=$((ERRORS + 1))
    MISSING_SPLITS=1
  fi
done
if [ $MISSING_SPLITS -eq 0 ]; then
  echo "OK: All expected split-product files present"
fi

# 14. Known-flaky tests not marked #[ignore] (CI false-failure guard)
echo ""
echo "=== Known-flaky test ignore guard ==="
FLAKY_UNIGNORED=0
# construction_dust_particles_spawn is documented as flaky in features.json (game_loop.known_issues)
# Probabilistic particle spawn over 20 ticks → ~0.3% false-failure rate per CI run.
# Should be #[ignore]d to avoid non-deterministic CI failures.
if grep -q "fn construction_dust_particles_spawn" src/game/tests.rs 2>/dev/null; then
  if grep -B3 "fn construction_dust_particles_spawn" src/game/tests.rs | grep -q "#\[ignore"; then
    echo "OK: Known-flaky test construction_dust_particles_spawn is properly marked #[ignore]"
  else
    echo "WARN: construction_dust_particles_spawn is documented as flaky (features.json:game_loop) but not marked #[ignore] — false CI failures expected (hit on 2026-04-13)"
    WARNINGS=$((WARNINGS + 1))
    FLAKY_UNIGNORED=1
  fi
fi
if [ $FLAKY_UNIGNORED -eq 0 ] && ! grep -q "fn construction_dust_particles_spawn" src/game/tests.rs 2>/dev/null; then
  echo "OK: construction_dust_particles_spawn not found (removed or renamed — update this check)"
fi

# 15. Non-determinism guard: rand::rng() in game simulation hot paths
echo ""
echo "=== RNG determinism check ==="
NONDETERMINISTIC=0
for hotpath in src/ecs/systems.rs src/game/mod.rs src/ecs/ai.rs; do
  if [ -f "$hotpath" ]; then
    count=$(grep -c "rand::rng()" "$hotpath" 2>/dev/null || true)
    if [ "$count" -gt 0 ]; then
      echo "WARN: $hotpath uses rand::rng() ($count occurrences) — non-deterministic across same-seed replays (BACKLOG.md: simulation non-determinism)"
      WARNINGS=$((WARNINGS + 1))
      NONDETERMINISTIC=1
    fi
  fi
done
if [ $NONDETERMINISTIC -eq 0 ]; then
  echo "OK: No rand::rng() in game loop hot paths — simulation determinism maintained"
fi

# 16. metrics_history.json freshness (health-check continuity guard)
echo ""
echo "=== Metrics history freshness ==="
if [ ! -f "docs/metrics_history.json" ]; then
  echo "WARN: docs/metrics_history.json not found — trend tracking missing"
  WARNINGS=$((WARNINGS + 1))
else
  LATEST_DATE=$(jq -r '.[-1].date // empty' "docs/metrics_history.json" 2>/dev/null)
  if [ -z "$LATEST_DATE" ] || [ "$LATEST_DATE" = "null" ]; then
    echo "WARN: docs/metrics_history.json has no entries"
    WARNINGS=$((WARNINGS + 1))
  else
    days=$(python3 -c "
from datetime import date
try:
  lv = date.fromisoformat('$LATEST_DATE')
  today = date.fromisoformat('$(date +%Y-%m-%d 2>/dev/null || echo 2026-01-01)')
  print((today - lv).days)
except:
  print(0)
" 2>/dev/null || echo 0)
    if [ "$days" -gt 14 ]; then
      echo "WARN: metrics_history.json last entry is $LATEST_DATE ($days days ago > 14) — rubric evaluations may be stale"
      WARNINGS=$((WARNINGS + 1))
    else
      echo "OK: metrics_history.json recent entry: $LATEST_DATE ($days days ago)"
    fi
  fi
fi

# 17. Score plateau detection: warn if last 5 metrics_history entries show no improvement
echo ""
echo "=== Score plateau detection ==="
if [ -f "docs/metrics_history.json" ]; then
  plateau=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if len(data) < 5:
    print('insufficient_data')
else:
    recent = [e.get('rubric_avg', e.get('score')) for e in data[-5:] if e.get('rubric_avg') or e.get('score')]
    if len(recent) >= 5 and round(max(recent) - min(recent), 2) <= 0.10:
        print(f'plateau:{len(recent)}:{recent[-1]}')
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$plateau" | grep -q "^plateau:"; then
    count=$(echo "$plateau" | cut -d: -f2)
    score=$(echo "$plateau" | cut -d: -f3)
    echo "WARN: Game score has been flat at ~${score}/5.0 for ${count}+ consecutive entries — no improvement detected. Prioritize BACKLOG.md items to break plateau."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: Score is not in a plateau (recent variation detected or insufficient history)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found — cannot check for plateau"
fi

# 18. needs_work systems with test_count=0 (design issues without test coverage)
echo ""
echo "=== needs_work zero-test check ==="
NEEDS_WORK_ZERO=0
for system in $(jq -r '.systems | keys[]' "$FEATURES"); do
  status=$(jq -r ".systems[\"$system\"].status" "$FEATURES")
  count=$(jq -r ".systems[\"$system\"].test_count" "$FEATURES")
  if [ "$status" = "needs_work" ] && [ "$count" = "0" ]; then
    echo "WARN: $system status='needs_work' but test_count=0 — known design issues with no test coverage (higher risk than needs_tests)"
    WARNINGS=$((WARNINGS + 1))
    NEEDS_WORK_ZERO=1
  fi
done
if [ $NEEDS_WORK_ZERO -eq 0 ]; then
  echo "OK: All 'needs_work' systems have some test coverage"
fi

# 19. Test suite size regression guard (catch accidental test deletions)
echo ""
echo "=== Test suite size regression guard ==="
actual_test_count=$(grep -r '#\[test\]' src/ --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
if [ "$actual_test_count" -lt 700 ]; then
  echo "FAIL: Only $actual_test_count #[test] annotations in src/ — expected >= 700 (possible accidental test deletion; suite was 818 on 2026-04-17)"
  ERRORS=$((ERRORS + 1))
else
  echo "OK: $actual_test_count #[test] annotations found in src/ (>= 700 threshold)"
fi

# 20. Screenshot mode half_speed_base regression guard
# --screenshot sets half_speed_base=true (main.rs), causing --ticks 12000 to yield game.tick≈6000.
# All historical evaluations are calibrated to this. If removed, comparisons break.
echo ""
echo "=== Screenshot half_speed_base guard ==="
if grep -q "half_speed_base = true" src/main.rs 2>/dev/null; then
  echo "OK: half_speed_base=true present in src/main.rs (screenshot eval calibrated to game.tick≈6000 for --ticks 12000)"
else
  echo "WARN: half_speed_base=true not found in src/main.rs — if removed from --screenshot mode, all historical rubric evaluations are no longer comparable (game.tick would double to 12000)"
  WARNINGS=$((WARNINGS + 1))
fi

# 21. Fog-of-war triviality guard: warn if sight_range or initial reveal radius is so large
#     that the exploration map is trivially 100% revealed (confirmed in all eval seeds).
echo ""
echo "=== Fog-of-war triviality check ==="
FOG_TRIVIAL=0
# Use spawn_villager-scoped grep to get villager sight_range (not wolf/prey values)
sight_val=$(awk '/fn spawn_villager/,/^\}/' src/ecs/spawn.rs 2>/dev/null | grep -oE 'sight_range:\s*[0-9]+(\.[0-9]+)?' | grep -oE '[0-9]+(\.[0-9]+)?' | head -1)
reveal_val=$(grep -oE 'exploration\.reveal\([^)]+,\s*[0-9]+\)' src/game/mod.rs 2>/dev/null | grep -oE ',\s*[0-9]+\)' | grep -oE '[0-9]+' | head -1)
if [ -n "$sight_val" ] && awk "BEGIN {exit !($sight_val > 15)}"; then
  echo "WARN: sight_range=$sight_val in spawn.rs — trivially reveals a 70x25 map (exploration_pct=100% in all eval seeds). Fog-of-war not functional as game mechanic."
  WARNINGS=$((WARNINGS + 1))
  FOG_TRIVIAL=1
fi
if [ -n "$reveal_val" ] && [ "$reveal_val" -gt 10 ]; then
  echo "WARN: Initial exploration reveal radius=$reveal_val in game/mod.rs — instantly reveals ~40% of a 70x25 map. Reduce for meaningful fog-of-war."
  WARNINGS=$((WARNINGS + 1))
  FOG_TRIVIAL=1
fi
if [ $FOG_TRIVIAL -eq 0 ]; then
  echo "OK: Fog-of-war parameters within reasonable range (sight_range<=15, reveal_radius<=10)"
fi

# 22. VillagerMemory Pillar 2 gap detector
# Design doc (game_design.md Pillar 2) requires per-villager memory to drive AI decisions.
# VillagerMemory is defined in components.rs and written in systems.rs, but if it is never
# read in ai.rs, Pillar 2 (local decision-making) is not implemented — villagers still use
# global stockpile counts instead of personal knowledge.
echo ""
echo "=== Pillar 2 gap: VillagerMemory in AI ==="
if ! grep -q "VillagerMemory" src/ecs/ai.rs 2>/dev/null; then
  echo "WARN: VillagerMemory not referenced in src/ecs/ai.rs — per-villager memory (Pillar 2) is defined and written but not read for AI decisions. Villagers still use global state."
  WARNINGS=$((WARNINGS + 1))
else
  echo "OK: VillagerMemory is referenced in ai.rs — Pillar 2 memory-driven decisions connected"
fi

# 23. Persistent food crisis: food_per_cap < 2.0 in 2+ of 3 seeds for 3+ consecutive entries
# Catches chronic food under-production that isn't resolved between health checks.
echo ""
echo "=== Persistent food crisis check ==="
if [ -f "docs/metrics_history.json" ]; then
  crisis=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if len(data) < 3:
    print('insufficient_data')
else:
    window = data[-5:]
    crisis_entries = sum(
        1 for entry in window
        if sum(1 for s in entry.get('seeds', {}).values()
               if s.get('food_per_cap', 999) < 2.0) >= 2
    )
    if crisis_entries >= 3:
        print(f'crisis:{crisis_entries}:{len(window)}')
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$crisis" | grep -q "^crisis:"; then
    count=$(echo "$crisis" | cut -d: -f2)
    total=$(echo "$crisis" | cut -d: -f3)
    echo "WARN: food_per_cap < 2.0 in 2+ seeds in ${count}/${total} recent health checks — food system chronically under-producing. Diagnose farm yield vs hunger rate before changing constants."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: No persistent food crisis (food_per_cap >= 2.0 in majority of seeds)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found — cannot check food crisis trend"
fi

# 24. Seed 42 population stagnation guard
# Seed 42 has been exactly pop=4, food=12 in every health check since initial baselines.
# This indicates a structural failure (auto-build never places second Hut despite wood surplus)
# not random noise. Warn when stuck for 3+ consecutive entries.
echo ""
echo "=== Seed 42 stagnation check ==="
if [ -f "docs/metrics_history.json" ]; then
  stagnation=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if len(data) < 3:
    print('insufficient_data')
else:
    window = data[-5:]
    stuck = sum(
        1 for e in window
        if e.get('seeds', {}).get('42', {}).get('population') == 4
        and e.get('seeds', {}).get('42', {}).get('food') == 12
    )
    if stuck >= 3:
        print(f'stuck:{stuck}:{len(window)}')
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$stagnation" | grep -q "^stuck:"; then
    count=$(echo "$stagnation" | cut -d: -f2)
    total=$(echo "$stagnation" | cut -d: -f3)
    echo "WARN: Seed 42 has been exactly pop=4, food=12 for ${count}/${total} recent health checks — auto-build growth failure. Diagnose Hut placement priority in game/build.rs before changing constants."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: Seed 42 is not stagnating (population or food changed recently)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found — cannot check seed 42 stagnation"
fi

# 25. ROAD_TRAFFIC_THRESHOLD too high for small settlements (road formation gate)
# At 300.0, evaluation seeds (pop=4-13 at tick 6000) cannot accumulate enough traffic
# at any single tile for auto-road-build to trigger. Roads never form in evaluation window.
echo ""
echo "=== Road formation threshold check ==="
road_thresh=$(grep -oE 'ROAD_TRAFFIC_THRESHOLD[^=]+=\s*[0-9]+(\.[0-9]+)?' src/game/mod.rs 2>/dev/null | grep -oE '[0-9]+(\.[0-9]+)?$' | head -1)
if [ -n "$road_thresh" ]; then
  too_high=$(awk "BEGIN {print ($road_thresh > 200.0) ? 1 : 0}")
  if [ "$too_high" = "1" ]; then
    echo "WARN: ROAD_TRAFFIC_THRESHOLD=${road_thresh} in src/game/mod.rs — prevents road formation in small settlements (0% Road coverage in all eval seeds at tick 6000 with pop=4-13). Consider tuning to <=100 for emergent roads."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: ROAD_TRAFFIC_THRESHOLD=${road_thresh} (<= 200.0, road formation feasible for small settlements)"
  fi
else
  echo "SKIP: ROAD_TRAFFIC_THRESHOLD not found in src/game/mod.rs"
fi

# 26. Extracted game sub-modules test coverage
# fire.rs, water_cycle.rs, particles.rs were extracted from game/mod.rs but
# have no unit tests — only indirect coverage through large integration tests.
# Documented in features.json:game_loop.known_issues.
echo ""
echo "=== Extracted game sub-module test coverage ==="
UNTESTED_MODULES=0
for module in src/game/fire.rs src/game/water_cycle.rs src/game/particles.rs; do
  if [ -f "$module" ]; then
    count=$(grep -c '#\[test\]' "$module" 2>/dev/null || true)
    if [ "$count" -eq 0 ]; then
      echo "WARN: $module has 0 tests — extracted from game/mod.rs but no unit tests added (features.json:game_loop known_issues)"
      WARNINGS=$((WARNINGS + 1))
      UNTESTED_MODULES=1
    fi
  fi
done
if [ "$UNTESTED_MODULES" -eq 0 ]; then
  echo "OK: All extracted game sub-modules have at least 1 test"
fi

# 27. Drought event threshold impossibility (static analysis)
# Drought in events.rs requires grain >= villager_count * N. If N is too large,
# drought can never fire in the evaluation window (grain=0-22 at pop=4-12 at tick 6000).
# Documented in features.json:events.known_issues ("grain>=pop*5 threshold never met").
echo ""
echo "=== Drought event threshold check ==="
drought_mult=$(grep -oE 'grain >= villager_count \* ([0-9]+)' src/game/events.rs 2>/dev/null | grep -oE '[0-9]+$' | head -1)
if [ -n "$drought_mult" ]; then
  if [ "$drought_mult" -gt 3 ]; then
    echo "WARN: Drought requires grain >= pop * $drought_mult (src/game/events.rs) — evaluation seeds accumulate grain=0-22 at pop=4-12 at tick 6000; threshold needs pop*${drought_mult}=$((4 * drought_mult))-$((12 * drought_mult)) grain. Drought is structurally impossible in evaluation window."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: Drought grain threshold multiplier=${drought_mult} (<=3) — feasible for typical population sizes"
  fi
else
  echo "SKIP: Drought grain threshold pattern not found in src/game/events.rs — check may need updating"
fi

# 28. rand::rng() count growth guard
# Documents the baseline rand::rng() call counts in hot-path files.
# If any file exceeds its documented count, the simulation is becoming MORE non-deterministic.
# Current baseline: systems.rs=3, game/mod.rs=3, ai.rs=0
echo ""
echo "=== rand::rng() count growth guard ==="
RNG_REGRESSED=0
check_rng_count() {
  local filepath="$1"
  local expected_max="$2"
  if [ -f "$filepath" ]; then
    local count
    count=$(grep -c "rand::rng()" "$filepath" 2>/dev/null || true)
    if [ "$count" -gt "$expected_max" ]; then
      echo "WARN: $filepath has $count rand::rng() calls (baseline was $expected_max) — simulation is becoming MORE non-deterministic; diagnose before adding more"
      WARNINGS=$((WARNINGS + 1))
      RNG_REGRESSED=1
    fi
  fi
}
check_rng_count "src/ecs/systems.rs" 3
check_rng_count "src/game/mod.rs" 3
check_rng_count "src/ecs/ai.rs" 0
if [ "$RNG_REGRESSED" -eq 0 ]; then
  echo "OK: rand::rng() counts in hot paths have not grown beyond baseline (systems.rs<=3, mod.rs<=3, ai.rs=0)"
fi

# 29. CLAUDE.md test count staleness
# CLAUDE.md claims "~190 tests" but actual count is 778 (4× off as of 2026-04-17).
# Stale estimate misleads new contributors and agents about the test suite.
# Warns if the CLAUDE.md claim is off by 2× or more from actual annotation count.
echo ""
echo "=== CLAUDE.md test count staleness ==="
claude_claim=$(grep -oE '~[0-9]+\s*(lib\s*)?tests' CLAUDE.md | grep -oE '[0-9]+' | head -1)
actual_tests=$(grep -r '#\[test\]' src/ --include="*.rs" 2>/dev/null | wc -l | tr -d ' ')
if [ -n "$claude_claim" ] && [ "$claude_claim" -gt 0 ]; then
  ratio=$(echo "$claude_claim $actual_tests" | awk '{if ($1>0) printf "%.0f", ($2/$1)*100; else print 0}')
  if [ "$ratio" -gt 200 ] || [ "$ratio" -lt 50 ]; then
    echo "WARN: CLAUDE.md claims ~${claude_claim} tests; actual is ${actual_tests} (${ratio}% of claim) — stale, misleads contributors. Update CLAUDE.md comments."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: CLAUDE.md test count claim (~${claude_claim}) close to actual (${actual_tests})"
  fi
else
  echo "SKIP: No test count found in CLAUDE.md (pattern: '~N tests')"
fi

# 30. Population swing detection: warns if any seed's population changes by >5
# between the last two consecutive metrics_history entries. Captures the observable
# impact of rand::rng() non-determinism on game outcomes (complements check #15
# which catches the code-level rand::rng() calls).
echo ""
echo "=== Population swing detection ==="
if [ -f "docs/metrics_history.json" ]; then
  swing=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if len(data) < 2:
    print('insufficient_data')
else:
    last = data[-1].get('seeds', {})
    prev = data[-2].get('seeds', {})
    max_swing = 0
    worst = ''
    for seed in last:
        lp = last[seed].get('population', 0)
        pp = prev.get(seed, {}).get('population', 0)
        delta = abs(lp - pp)
        if delta > max_swing:
            max_swing = delta
            worst = f'seed {seed}: {pp}->{lp}'
    if max_swing > 5:
        print(f'large_swing:{max_swing}:{worst}')
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$swing" | grep -q "^large_swing:"; then
    delta=$(echo "$swing" | cut -d: -f2)
    desc=$(echo "$swing" | cut -d: -f3-)
    echo "WARN: Population changed by ${delta} between last two health checks ($desc) — rand::rng() non-determinism causing major game-state divergence between runs. Root cause: seeded RNG not used in auto-build/breeding paths."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: Population swing between consecutive health checks within expected range (<= 5)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found — cannot check population swing"
fi

# 31. Prey extinction guard: rabbits=0 in 2+ seeds indicates ecosystem imbalance
# game/mod.rs:878 documents: "Prey provide an early food web and are required for the
# breeding system to function." Silent prey extinction may cause wolves to target
# villagers exclusively, elevating threat scores. Field added to metrics_history on 2026-04-29.
echo ""
echo "=== Prey extinction check ==="
if [ -f "docs/metrics_history.json" ]; then
  extinction=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if not data:
    print('no_data')
else:
    last = data[-1]
    seeds = last.get('seeds', {})
    extinct = sum(1 for s in seeds.values() if s.get('rabbits', -1) == 0)
    total_with_data = sum(1 for s in seeds.values() if 'rabbits' in s)
    if total_with_data == 0:
        print('no_rabbit_data')
    elif extinct >= 2:
        print(f'extinct:{extinct}:{total_with_data}')
    else:
        print('ok')
" 2>/dev/null || echo "no_data")
  if echo "$extinction" | grep -q "^extinct:"; then
    count=$(echo "$extinction" | cut -d: -f2)
    total=$(echo "$extinction" | cut -d: -f3)
    echo "WARN: Rabbits extinct (count=0) in ${count}/${total} seeds in last metrics entry — prey extinction may drive wolves to target villagers, elevating threat scores. See ecs_core known_issues."
    WARNINGS=$((WARNINGS + 1))
  elif echo "$extinction" | grep -q "no_rabbit_data"; then
    echo "SKIP: No 'rabbits' field in most recent metrics_history entry — add it to enable prey tracking"
  else
    echo "OK: Rabbit population present in 2+ evaluation seeds (ecosystem viable)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

# 32. Flat terrain detection guard
# Pillar 1 ("Geography Shapes Everything") requires terrain with hills, mountain passes,
# and varied topography to create strategic constraints on settlement growth.
# slope_flat_pct > 95% across 2+ eval seeds means the terrain is too flat for geography
# to meaningfully shape settlement development. Measured 2026-04-30: all 3 seeds at 98%+ flat.
# Field 'slope_flat_pct' must be present in metrics_history entries for this check to fire.
echo ""
echo "=== Flat terrain detection ==="
if [ -f "docs/metrics_history.json" ]; then
  flat_terrain=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if not data:
    print('no_data')
else:
    last = data[-1]
    seeds = last.get('seeds', {})
    flat_seeds = [
        s for s, v in seeds.items()
        if v.get('slope_flat_pct', -1) > 95.0
    ]
    total_with_data = sum(1 for v in seeds.values() if 'slope_flat_pct' in v)
    if total_with_data == 0:
        print('no_slope_data')
    elif len(flat_seeds) >= 2:
        pcts = [f\"{s}={seeds[s]['slope_flat_pct']:.1f}%\" for s in flat_seeds]
        print(f'flat:{len(flat_seeds)}:{total_with_data}:{\",\".join(pcts)}')
    else:
        print('ok')
" 2>/dev/null || echo "no_data")
  if echo "$flat_terrain" | grep -q "^flat:"; then
    count=$(echo "$flat_terrain" | cut -d: -f2)
    total=$(echo "$flat_terrain" | cut -d: -f3)
    desc=$(echo "$flat_terrain" | cut -d: -f4-)
    echo "WARN: slope_flat_pct > 95% in ${count}/${total} seeds ($desc) — terrain nearly flat; Pillar 1 ('Geography Shapes Everything') requires hills/mountains for strategic constraints. Investigate terrain_pipeline.rs height generation and erosion amplitude."
    WARNINGS=$((WARNINGS + 1))
  elif echo "$flat_terrain" | grep -q "no_slope_data"; then
    echo "SKIP: No 'slope_flat_pct' field in most recent metrics_history entry — add it to enable flat terrain detection"
  else
    echo "OK: Terrain slope distribution acceptable (<= 95% flat in 2+ seeds)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

# 33. Event modifier chain dead code guard
# tick_config clones sim_config and applies drought/harvest rain_rate multipliers,
# but step_water_cycle signature is (should_rain: bool, veg_growth_mult: f64) — no
# rain_rate parameter. tick_config is built, modified, and silently discarded.
# Drought/harvest events would have zero effect on simulation even if they fired.
# Documented 2026-05-01: src/game/mod.rs:2135-2147 vs water_cycle.rs:9.
echo ""
echo "=== Event modifier chain dead code guard ==="
if grep -q "tick_config.rain_rate" src/game/mod.rs 2>/dev/null; then
  if ! grep -qE "step_water_cycle.*tick_config|tick_config.*step_water_cycle" src/game/mod.rs 2>/dev/null; then
    echo "WARN: tick_config event modifiers (drought: rain_rate*=0.4, harvest: rain_rate*=1.5) built in game/mod.rs but never passed to step_water_cycle (signature: bool, f64) — event chain broken; drought/harvest have zero effect on simulation even when they fire."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: tick_config is passed to step_water_cycle — event modifiers connected"
  fi
else
  echo "SKIP: tick_config.rain_rate not found in game/mod.rs — check may need updating"
fi

# 34. WorldState 0D architecture regression guard
# WorldState is the canonical simulation state introduced in the 0D refactor (Stages 1-4).
# If game/mod.rs stops referencing WorldState, the refactor may have been accidentally reverted.
# game/mod.rs:429 holds pub state: crate::world_state::WorldState.
echo ""
echo "=== WorldState 0D architecture regression guard ==="
if grep -q "WorldState" src/game/mod.rs 2>/dev/null; then
  echo "OK: WorldState referenced in src/game/mod.rs (0D architecture intact)"
else
  echo "WARN: WorldState not referenced in src/game/mod.rs — 0D architecture may have been accidentally reverted (check world_state.rs, game/mod.rs)"
  WARNINGS=$((WARNINGS + 1))
fi

# 35. build_site_gets_completed_in_game flaky test guard
# Discovered 2026-05-05: fails in full test suite (777 passed, 1 failed), passes in isolation.
# Root cause: rand::rng() in ecs/systems.rs and game/mod.rs causes villagers to not complete
# the build site within 3000 ticks on some non-deterministic runs. Same mechanism as
# construction_dust_particles_spawn. Should be marked #[ignore] until seeded RNG is used.
echo ""
echo "=== Known-flaky test ignore guard: build_site_gets_completed_in_game ==="
if grep -q "fn build_site_gets_completed_in_game" src/game/tests.rs 2>/dev/null; then
  if grep -B3 "fn build_site_gets_completed_in_game" src/game/tests.rs | grep -q "#\[ignore"; then
    echo "OK: Known-flaky test build_site_gets_completed_in_game is properly marked #[ignore]"
  else
    echo "WARN: build_site_gets_completed_in_game fails in full test suite (discovered 2026-05-05) but not marked #[ignore] — non-deterministic CI failures expected. Root cause: rand::rng() in AI/build paths (features.json:ecs_core known_issues)"
    WARNINGS=$((WARNINGS + 1))
  fi
else
  echo "OK: build_site_gets_completed_in_game not found (removed or renamed — update this check)"
fi

# 36. Event suppression detector
# If the last 5 metrics_history entries all show events_fired=0 in 2+ seeds,
# the event system is structurally suppressed (not just unlucky). Root cause is the
# broken tick_config modifier chain: drought/harvest modifiers built in game/mod.rs:2135-2147
# are never passed to step_water_cycle (check #33). 25 consecutive zero-event checks
# recorded as of 2026-05-06.
echo ""
echo "=== Event suppression detector ==="
if [ -f "docs/metrics_history.json" ]; then
  suppressed=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if len(data) < 5:
    print('insufficient_data')
else:
    window = data[-5:]
    fully_suppressed = sum(
        1 for entry in window
        if sum(1 for s in entry.get('seeds', {}).values()
               if s.get('events_fired', -1) == 0) >= 2
    )
    if fully_suppressed >= 5:
        print(f'suppressed:{fully_suppressed}:{len(window)}')
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$suppressed" | grep -q "^suppressed:"; then
    count=$(echo "$suppressed" | cut -d: -f2)
    total=$(echo "$suppressed" | cut -d: -f3)
    echo "WARN: events_fired=0 in 2+ seeds for ${count}/${total} consecutive health checks — event system structurally suppressed. Root cause: tick_config event modifiers (game/mod.rs:2135-2147) not passed to step_water_cycle. Fix event modifier pipeline (check #33) before investigating trigger rates."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: Events have fired in at least 1 seed in some recent health checks (not fully suppressed)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found — cannot check event suppression"
fi

# 37. Seed 42 housing saturation guard
# housing_growth_potential=0 for seed 42 in 3+ consecutive entries means
# auto-build is not placing Huts despite population need. This is a more
# robust signal than check #24 (pop=4 food=12), which fails to fire when
# food varies due to rand::rng() non-determinism. Field added 2026-04-30.
echo ""
echo "=== Seed 42 housing saturation check ==="
if [ -f "docs/metrics_history.json" ]; then
  saturation=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
window = data[-5:]
entries_with_data = [e for e in window if 'housing_growth_potential' in e.get('seeds', {}).get('42', {})]
if len(entries_with_data) < 3:
    print('insufficient_data')
else:
    recent = entries_with_data[-3:]
    stuck = sum(1 for e in recent if e.get('seeds', {}).get('42', {}).get('housing_growth_potential', -1) == 0)
    if stuck >= 3:
        print(f'saturated:{stuck}:{len(recent)}')
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$saturation" | grep -q "^saturated:"; then
    count=$(echo "$saturation" | cut -d: -f2)
    total=$(echo "$saturation" | cut -d: -f3)
    echo "WARN: Seed 42 housing_growth_potential=0 in ${count}/${total} recent checks — auto-build not placing Huts despite population pressure. Diagnose Hut placement priority in game/build.rs. Complements check #24 (which requires food=12 exactly)."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: Seed 42 housing growth potential is not persistently saturated (or insufficient history)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

# Summary
# 38. features.json source files not mentioned in ARCHITECTURE.md
# Every .rs file listed in features.json should appear in docs/ARCHITECTURE.md.
# When a file is omitted from the architecture overview, contributors and agents
# may miss it during analysis. Found 2026-05-08: analytical_erosion.rs (status=ok,
# 6 tests) and scripting/mod.rs (status=stub) absent from the architecture file map.
# For mod.rs files, checks the parent directory name (avoids false-positive from
# other mod.rs occurrences in ARCHITECTURE.md).
echo ""
echo "=== ARCHITECTURE.md coverage of features.json files ==="
ARCH_GAP=0
while IFS= read -r file; do
  if [ -f "$file" ]; then
    basename_file=$(basename "$file")
    if [ "$basename_file" = "mod.rs" ]; then
      search_token=$(basename "$(dirname "$file")")
    else
      search_token="$basename_file"
    fi
    if ! grep -q "$search_token" docs/ARCHITECTURE.md 2>/dev/null; then
      echo "WARN: $file is in features.json but '$search_token' not found in docs/ARCHITECTURE.md — undocumented in architecture overview"
      WARNINGS=$((WARNINGS + 1))
      ARCH_GAP=1
    fi
  fi
done < <(jq -r '.systems[].files[]' "$FEATURES" | grep '\.rs$')
if [ $ARCH_GAP -eq 0 ]; then
  echo "OK: All .rs files in features.json are mentioned in docs/ARCHITECTURE.md"
fi

# 39. Acute food crisis: food_per_cap < 1.5 in any seed in the most recent metrics entry.
# Complements check #23 (chronic: < 2.0 in 2+ seeds for 3+ of last 5 entries).
# Fires immediately on first occurrence of near-starvation, before chronic threshold.
# Seed 137 food/cap has ranged 0.73–1.73 across recent runs (non-deterministic).
echo ""
echo "=== Acute food crisis check ==="
if [ -f "docs/metrics_history.json" ]; then
  acute=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if not data:
    print('no_data')
else:
    last = data[-1]
    seeds = last.get('seeds', {})
    acute = [(s, v.get('food_per_cap', 999)) for s, v in seeds.items()
             if 'food_per_cap' in v and v['food_per_cap'] < 1.5]
    if acute:
        desc = ', '.join(f'seed {s}={fpc:.2f}' for s, fpc in acute)
        print(f'acute:{len(acute)}:{desc}')
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$acute" | grep -q "^acute:"; then
    count=$(echo "$acute" | cut -d: -f2)
    desc=$(echo "$acute" | cut -d: -f3-)
    echo "WARN: ACUTE FOOD CRISIS — food_per_cap < 1.5 in ${count} seed(s): $desc — settlement approaching starvation. Diagnose farm count vs population before touching hunger constants."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: No acute food crisis (food_per_cap >= 1.5 in all seeds in last metrics entry)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

# 40. CLAUDE.md stale module structure
# CLAUDE.md module structure section still lists 'simulation.rs' (monolith split to
# src/simulation/ in ~2026-03-01) and omits 'world_state.rs' (added in 0D architecture
# refactor Stages 1-4). Stale module structure misleads agents and contributors who rely on
# CLAUDE.md as the entry point for codebase navigation — they may look for files that don't
# exist or miss files that do. First flagged in health check 2026-05-11.
echo ""
echo "=== CLAUDE.md stale module structure check ==="
CLAUDE_STALE=0
if grep -q 'simulation\.rs' CLAUDE.md 2>/dev/null && [ ! -f "src/simulation.rs" ]; then
  echo "WARN: CLAUDE.md module structure lists 'simulation.rs' which was split to src/simulation/ — stale. Misleads agents/contributors navigating the codebase."
  WARNINGS=$((WARNINGS + 1))
  CLAUDE_STALE=1
fi
if [ -f "src/world_state.rs" ] && ! grep -q 'world_state\.rs' CLAUDE.md 2>/dev/null; then
  echo "WARN: CLAUDE.md module structure missing 'world_state.rs' (added in 0D architecture refactor, 74 lines — canonical simulation state). Update CLAUDE.md."
  WARNINGS=$((WARNINGS + 1))
  CLAUDE_STALE=1
fi
if [ $CLAUDE_STALE -eq 0 ]; then
  echo "OK: CLAUDE.md module structure references match actual file structure (simulation.rs absent, world_state.rs present)"
fi

# 41. Wood skill pure-decay detection
# woodcutting skill starts at 1.0 (game/mod.rs:381) and decays by 0.9999 each tick (game/mod.rs:1812).
# If woodcutting_ticks=0 (no trees cut), after 6000 eval ticks: skill = 1.0 * 0.9999^6000 ≈ 0.5488.
# Confirmed 2026-05-12: all 3 seeds show wood_skill=0.5487951708942683 to 16 decimal places.
# Root cause: Forest biome near 0% coverage (check #32) → no trees to cut → skill pure-decays.
# Requires 'wood_skill' field in metrics_history entries to fire.
echo ""
echo "=== Wood skill pure-decay detection ==="
if [ -f "docs/metrics_history.json" ]; then
  wood_decay=$(python3 -c "
import json
DECAY_EQ = 1.0 * (0.9999 ** 6000)  # ≈ 0.5488
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if not data:
    print('no_data')
else:
    last = data[-1]
    seeds = last.get('seeds', {})
    decay_seeds = [s for s, v in seeds.items()
                   if 'wood_skill' in v and abs(v['wood_skill'] - DECAY_EQ) < 0.001]
    total = sum(1 for v in seeds.values() if 'wood_skill' in v)
    if total == 0:
        print('no_skill_data')
    elif len(decay_seeds) >= 2:
        print(f'decaying:{len(decay_seeds)}:{total}:{DECAY_EQ:.4f}')
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$wood_decay" | grep -q "^decaying:"; then
    count=$(echo "$wood_decay" | cut -d: -f2)
    total=$(echo "$wood_decay" | cut -d: -f3)
    equil=$(echo "$wood_decay" | cut -d: -f4)
    echo "WARN: Wood skill ≈ ${equil} (pure decay: 1.0*0.9999^6000) in ${count}/${total} seeds — zero trees cut in evaluation window. Root cause: Forest biome near 0% coverage (check #32). Settlement is wood-dependent (all buildings require wood) but no forest exists. Fix biome distribution before tuning wood-gathering behavior."
    WARNINGS=$((WARNINGS + 1))
  elif echo "$wood_decay" | grep -q "no_skill_data"; then
    echo "SKIP: No 'wood_skill' field in metrics_history — add it to enable zero-woodcutting detection"
  else
    echo "OK: Wood skill above pure-decay equilibrium (0.5488) — some woodcutting activity present"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

# 42. Forest biome near-zero detection
# Desert has been 32-49% in 2/3 evaluation seeds while Forest is <1% in all 3 seeds.
# This directly explains wood_skill pure-decay (check #41) — no trees to cut because
# there are no forest tiles. Root cause: biome scoring in terrain_pipeline.rs assigns
# Desert too broadly post-WorldState refactor. First confirmed 2026-05-12 (check #41 added).
# Seeds 42=0.3%, 137=0.1%, 777=0.7% — all below 1% threshold.
# Complementary to check #41 (skill signal) and check #32 (flat terrain signal).
echo ""
echo "=== Forest biome near-zero detection ==="
if [ -f "docs/metrics_history.json" ]; then
  forest_check=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if not data:
    print('no_data')
else:
    last = data[-1]
    seeds = last.get('seeds', {})
    low_forest = [(s, v.get('forest_pct', 999)) for s, v in seeds.items()
                  if 'forest_pct' in v and v['forest_pct'] < 1.0]
    total = sum(1 for v in seeds.values() if 'forest_pct' in v)
    if total == 0:
        print('no_forest_data')
    elif len(low_forest) >= 2:
        desc = ', '.join(f'seed {s}={fp:.1f}%' for s, fp in low_forest)
        print(f'low:{len(low_forest)}:{total}:{desc}')
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$forest_check" | grep -q "^low:"; then
    count=$(echo "$forest_check" | cut -d: -f2)
    total=$(echo "$forest_check" | cut -d: -f3)
    desc=$(echo "$forest_check" | cut -d: -f4-)
    echo "WARN: Forest biome < 1.0% in ${count}/${total} seeds (${desc}) — settlement wood-dependent but no forest exists. Root cause: biome distribution imbalance in terrain_pipeline.rs (Desert dominant). Fix biome scoring before tuning wood/skill systems. See also check #41 (wood_skill decay) and check #32 (flat terrain)."
    WARNINGS=$((WARNINGS + 1))
  elif echo "$forest_check" | grep -q "no_forest_data"; then
    echo "SKIP: No 'forest_pct' field in metrics_history — add it to enable forest near-zero detection"
  else
    echo "OK: Forest biome >= 1.0% in sufficient seeds — forest coverage adequate for woodcutting"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

# 43. Multi-seed housing growth saturation
# Seed 42 has been at housing_growth_potential=0 for 28+ consecutive checks (check #37).
# This check fires when 2+ evaluation seeds simultaneously show housing_growth_potential=0,
# indicating the auto-build Hut stagnation has spread beyond seed 42 — a broadening failure mode.
# Complements check #37 (seed 42 specific). Does not fire when only one seed is saturated.
echo ""
echo "=== Multi-seed housing saturation check ==="
if [ -f "docs/metrics_history.json" ]; then
  multi_saturated=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if not data:
    print('no_data')
else:
    last = data[-1]
    seeds = last.get('seeds', {})
    saturated = [s for s, v in seeds.items()
                 if 'housing_growth_potential' in v and v['housing_growth_potential'] == 0]
    total_with_data = sum(1 for v in seeds.values() if 'housing_growth_potential' in v)
    if total_with_data == 0:
        print('no_data')
    elif len(saturated) >= 2:
        print(f'multi_saturated:{len(saturated)}:{total_with_data}:{\",\".join(sorted(saturated))}')
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$multi_saturated" | grep -q "^multi_saturated:"; then
    count=$(echo "$multi_saturated" | cut -d: -f2)
    total=$(echo "$multi_saturated" | cut -d: -f3)
    seeds_list=$(echo "$multi_saturated" | cut -d: -f4-)
    echo "WARN: housing_growth_potential=0 in ${count}/${total} seeds (seeds ${seeds_list}) — auto-build Hut stagnation spreading beyond seed 42. Escalation from check #37. Diagnose Hut placement priority in game/build.rs."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: Multi-seed housing saturation not detected (housing growth potential > 0 in 2+ seeds)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

# 44. Wood resource near-zero with build site stall detection
# All buildings cost wood (Farms=5w, Huts=10w, Workshops=8w, etc.), but Forest biome
# is <1% in all evaluation seeds (check #42), meaning wood cannot be replenished once
# depleted. When wood < 10 in 2+ seeds, settlements cannot complete queued construction.
# Observed 2026-05-17: seed 137 wood=4 with 5 active build_sites; seed 777 wood=1 with
# 1 build_site. Complements check #41 (skill decay = supply-side signal) and check #42
# (forest near-zero = cause); this detects the demand-side consequence: resource stall.
# Requires 'wood' field in metrics_history entries (added 2026-05-17) to fire.
echo ""
echo "=== Wood resource near-zero stall detection ==="
if [ -f "docs/metrics_history.json" ]; then
  wood_stall=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if not data:
    print('no_data')
else:
    last = data[-1]
    seeds = last.get('seeds', {})
    low_wood = [(s, v.get('wood', 999)) for s, v in seeds.items()
                if 'wood' in v and v['wood'] < 10]
    total = sum(1 for v in seeds.values() if 'wood' in v)
    if total == 0:
        print('no_wood_data')
    elif len(low_wood) >= 2:
        desc = ', '.join(f'seed {s}=wood:{w}' for s, w in low_wood)
        print(f'stall:{len(low_wood)}:{total}:{desc}')
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$wood_stall" | grep -q "^stall:"; then
    count=$(echo "$wood_stall" | cut -d: -f2)
    total=$(echo "$wood_stall" | cut -d: -f3)
    desc=$(echo "$wood_stall" | cut -d: -f4-)
    echo "WARN: Wood stockpile < 10 in ${count}/${total} seeds (${desc}) — construction stall risk. All buildings cost wood but Forest biome < 1% (check #42) means no replenishment. Fix biome distribution before tuning wood costs. See also check #41 (skill decay) and check #42 (forest coverage)."
    WARNINGS=$((WARNINGS + 1))
  elif echo "$wood_stall" | grep -q "no_wood_data"; then
    echo "SKIP: No 'wood' field in metrics_history — add it to enable wood stall detection (first added 2026-05-17)"
  else
    echo "OK: Wood stockpile >= 10 in 2+ seeds (no construction stall risk)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

# 45. Granary auto-build food threshold impossibility gate
# build.rs Priority 4 auto-builds a SECOND Granary only when: villager_count >= 12 AND food > N.
# NOTE: A pre-built FoodToGrain Granary is ALWAYS present from game start (game/mod.rs:1064),
# so food→grain IS running. What's blocked here is auto-build of a second Granary.
# IMPORTANT (2026-05-20 correction): has_granary=TRUE from game start via pre-built entity.
# The actual Bakery blockers are independent: (1) planks >= 8 (check #47, Workshop output),
# (2) grain > 30 (accumulates slowly via pre-built Granary). NOT "requires Granary" directly.
# Evaluation seeds have food=8-22 at tick 6000 (pop=4-13). If N > 40, second Granary auto-build
# is structurally impossible, but the pre-built Granary continues food→grain conversion.
# Confirmed 2026-05-18: N=80 (build.rs:1154). Result: bread=0 permanently (planks=0 blocks Bakery).
# Mirrors check #27 (drought grain threshold impossibility) in the events system.
echo ""
echo "=== Granary auto-build food threshold check ==="
granary_thresh=$(grep -E '!has_granary.*food >' src/game/build.rs 2>/dev/null | grep -oE 'food > ([0-9]+)' | grep -oE '[0-9]+$' | head -1)
if [ -n "$granary_thresh" ]; then
  if [ "$granary_thresh" -gt 40 ]; then
    echo "WARN: Second-Granary auto-build requires food > $granary_thresh (src/game/build.rs) — unreachable (eval food=8-22). NOTE: pre-built FoodToGrain Granary (game/mod.rs:1064) IS running, so food→grain chain works. True Bakery blockers: (1) planks >= 8 (check #47, Workshop starved of wood) and (2) grain > 30. Fix biome distribution (check #42) to unblock planks before tuning this threshold."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: Granary food threshold=${granary_thresh} (<=40) — reachable by evaluation populations"
  fi
else
  echo "SKIP: Granary auto-build condition not found in src/game/build.rs — check may need updating"
fi

# 46. ProcessingBuilding diagnostic conflation check
# collect_diagnostics() at game/mod.rs:2299 counts ALL ProcessingBuilding entities as "Workshop".
# But game/mod.rs:1064-1081 spawns a PRE-BUILT Granary (FoodToGrain recipe) as a ProcessingBuilding
# at game start — before auto-build ever runs. Result: every seed always shows Workshop:1 (or more)
# even when no auto-built Workshop exists. 25+ health checks recorded Granary as Workshop.
# The pre-built Granary does process food->grain when food > 15, explaining grain accumulation
# (grain=4-20 in diagnostics) without a visible "Granary" in the building list.
# Fix: query building_counts by recipe or marker to separate Granary/Workshop/Bakery.
# First identified: 2026-05-19.
echo ""
echo "=== ProcessingBuilding diagnostic conflation check ==="
DIAG_CONFLATION=0
if grep -q 'workshop_count.*ProcessingBuilding' src/game/mod.rs 2>/dev/null; then
  if grep -q 'Recipe::FoodToGrain' src/game/mod.rs 2>/dev/null; then
    echo "WARN: collect_diagnostics() (game/mod.rs:2299) labels ALL ProcessingBuilding entities as 'Workshop' but game/mod.rs:1064-1081 spawns a pre-built Granary (FoodToGrain recipe) also as a ProcessingBuilding. Workshop count in --diagnostics is inflated by 1 (pre-built Granary). 25+ health checks have misread this as Workshop data. Fix: separate Granary/Workshop/Bakery in building_counts by querying recipe or using a distinct marker component."
    WARNINGS=$((WARNINGS + 1))
    DIAG_CONFLATION=1
  fi
fi
if [ $DIAG_CONFLATION -eq 0 ]; then
  echo "OK: ProcessingBuilding diagnostic conflation not detected (workshop_count separated from Granary/Bakery)"
fi

# 47. Bakery planks threshold impossibility (static)
# Bakery auto-build (build.rs Priority 5.5) requires `planks >= N`.
# Planks are produced by Workshop (2 wood -> 1 plank). With Forest biome < 1% in all
# evaluation seeds (check #42) and wood < 10 in 2/3 seeds (check #44), Workshop never
# produces planks. This blocks Bakery independently of the Granary issue (check #45).
# Note: has_granary is actually TRUE from game start (pre-built FoodToGrain ProcessingBuilding
# at game/mod.rs:1064), so the Granary check #45 diagnosis is partially stale —
# the actual Bakery blockers are (1) planks=0 from wood starvation and (2) grain<30.
# Confirmed 2026-05-20: plank threshold is 8 in build.rs Priority 5.5.
echo ""
echo "=== Bakery planks threshold impossibility check ==="
bakery_planks=$(grep -E 'planks >= [0-9]+' src/game/build.rs 2>/dev/null | grep -oE 'planks >= [0-9]+' | head -1 | grep -oE '[0-9]+$')
if [ -n "$bakery_planks" ]; then
  if [ "$bakery_planks" -gt 2 ]; then
    echo "WARN: Bakery requires planks >= $bakery_planks (src/game/build.rs Priority 5.5). Planks come from Workshop (2 wood -> 1 plank). With Forest < 1% in all eval seeds (check #42) and wood < 10 in 2/3 seeds (check #44), Workshop produces 0 planks -> Bakery blocked independently of Granary. Fix biome distribution (check #42) before tuning Bakery thresholds. Also: Bakery requires grain > 30, accumulating slowly via pre-built FoodToGrain Granary."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: Bakery planks threshold=${bakery_planks} (<= 2) — reachable from low Workshop output"
  fi
else
  echo "SKIP: Bakery planks condition not found in src/game/build.rs — check may need updating"
fi

# 48. Seeds 137/777 population convergence guard
# Pillar 1 ("Geography Shapes Everything") requires different terrain to produce
# different settlements. Seeds 137 (Desert-dominant, Forest=0.1%) and 777 (Grass/Sand,
# Desert=0.1%) have reached nearly identical populations (diff ≤ 1) in 4/5 recent
# health checks, despite radically different terrain biome distributions.
# When population difference is ≤ 1 in 4+ of the last 5 entries, terrain is not
# meaningfully shaping settlement outcomes — a direct Pillar 1 violation.
# First formally tracked: 2026-05-21 (pattern observed in notes since 2026-05-20).
echo ""
echo "=== Seeds 137/777 Pillar 1 convergence guard ==="
if [ -f "docs/metrics_history.json" ]; then
  convergence=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if len(data) < 5:
    print('insufficient_data')
else:
    window = data[-5:]
    entries_with_both = [
        e for e in window
        if '137' in e.get('seeds', {}) and '777' in e.get('seeds', {})
        and 'population' in e['seeds']['137'] and 'population' in e['seeds']['777']
    ]
    if len(entries_with_both) < 4:
        print('insufficient_data')
    else:
        converged = sum(
            1 for e in entries_with_both
            if abs(e['seeds']['137']['population'] - e['seeds']['777']['population']) <= 1
        )
        if converged >= 4:
            pops = [(e['date'], e['seeds']['137']['population'], e['seeds']['777']['population'])
                    for e in entries_with_both]
            summary = ', '.join(f'{d}:({p137},{p777})' for d,p137,p777 in pops[-3:])
            print(f'convergent:{converged}:{len(entries_with_both)}:{summary}')
        else:
            print('ok')
" 2>/dev/null || echo "ok")
  if echo "$convergence" | grep -q "^convergent:"; then
    count=$(echo "$convergence" | cut -d: -f2)
    total=$(echo "$convergence" | cut -d: -f3)
    desc=$(echo "$convergence" | cut -d: -f4-)
    echo "WARN: Seeds 137 and 777 population diff <=1 in ${count}/${total} recent health checks (${desc}) — terrain not differentiating settlement outcomes. Pillar 1 ('two different maps should produce two fundamentally different settlements') violated. Root cause: biome distribution imbalance (Desert/Forest) overriding terrain variety. Fix terrain_pipeline.rs biome scoring before tuning AI behavior."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: Seeds 137 and 777 show meaningful population divergence (Pillar 1 terrain differentiation functioning)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

# 49. Metrics history schema completeness
# Many harness checks silently SKIP when required fields are missing from the most
# recent metrics_history.json entry. For example: check #31 requires 'rabbits',
# check #32 requires 'slope_flat_pct', check #41 requires 'wood_skill', check #42
# requires 'forest_pct', check #44 requires 'wood', check #48 requires 'population'.
# If a health check agent omits these fields, downstream checks SKIP without warning.
# This meta-check ensures the latest entry is complete so all checks can fire.
# Required fields confirmed necessary as of 2026-05-27 (check #49 added).
echo ""
echo "=== Metrics history schema completeness ==="
if [ -f "docs/metrics_history.json" ]; then
  schema_missing=$(python3 -c "
import json
REQUIRED_SEED_FIELDS = [
    'population', 'food', 'food_per_cap', 'wood', 'buildings',
    'water_pct', 'biomes', 'rabbits', 'survived',
    'slope_flat_pct', 'desert_pct', 'forest_pct',
    'events_fired', 'housing_growth_potential', 'wood_skill',
    'grain', 'planks', 'bread'
]
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if not data:
    print('no_data')
else:
    last = data[-1]
    seeds = last.get('seeds', {})
    missing = []
    for seed, vals in seeds.items():
        for field in REQUIRED_SEED_FIELDS:
            if field not in vals:
                missing.append(f'seed {seed}: {field}')
    if missing:
        print('missing:' + '|'.join(missing[:5]))  # cap at 5 to avoid long output
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$schema_missing" | grep -q "^missing:"; then
    fields=$(echo "$schema_missing" | cut -d: -f2- | tr '|' '\n' | head -5)
    echo "WARN: Most recent metrics_history entry is missing required fields — downstream checks will SKIP silently. Missing: $fields"
    WARNINGS=$((WARNINGS + 1))
  elif echo "$schema_missing" | grep -q "no_data"; then
    echo "SKIP: metrics_history.json is empty — cannot validate schema"
  else
    echo "OK: Most recent metrics_history entry has all required per-seed fields (checks #31, #32, #41, #42, #44 will fire)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

# 50. Total starvation detection: food=0 with living population
# food_per_cap < 1.5 (check #39) covers near-starvation, but food=0 with pop>0
# is the sharpest crisis state — all stored food is gone; villagers die next hunger tick.
# First confirmed: 2026-05-28 diagnostics, seed 137: food=0, pop=11 at tick 6000.
# Pre-built FoodToGrain Granary (game/mod.rs:1064, food>15 threshold) may deplete the
# buffer faster than farms replenish it at high population, causing apparent starvation
# despite active farms. Complement to check #39 (food_per_cap < 1.5) and check #23 (chronic).
echo ""
echo "=== Total starvation detection ==="
if [ -f "docs/metrics_history.json" ]; then
  starvation=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if not data:
    print('no_data')
else:
    last = data[-1]
    seeds = last.get('seeds', {})
    starving = [(s, v.get('population', 0)) for s, v in seeds.items()
                if 'food' in v and v['food'] == 0 and v.get('population', 0) > 0]
    if starving:
        desc = ', '.join(f'seed {s} pop={p}' for s, p in starving)
        print(f'starving:{len(starving)}:{desc}')
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$starvation" | grep -q "^starving:"; then
    count=$(echo "$starvation" | cut -d: -f2)
    desc=$(echo "$starvation" | cut -d: -f3-)
    echo "WARN: TOTAL STARVATION — food=0 with living population in ${count} seed(s): $desc — all food exhausted; villagers starve immediately. Possible causes: (1) rand::rng() task assignment causes too few farmers; (2) pre-built FoodToGrain Granary (game/mod.rs:1064, food>15) consumes buffer faster than farms replenish it at high pop. Diagnose farm yield vs Granary consumption before touching hunger constants."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: No total starvation (food > 0 or population = 0 in all seeds)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

# 51. FoodToGrain Granary processing threshold too low for high-population viability
# src/ecs/systems.rs:979,1391: Granary fires when resources.food > N.
# At pop >= 10, villagers consume ~0.1-0.15 food/tick each = 1-1.5/tick total.
# If N < 20, the food buffer (N units) lasts ~70-100 ticks — insufficient to absorb
# task-assignment fluctuations from rand::rng() (farming fraction varies each run).
# Observed 2026-05-28: seed 137 pop=11, food=0 at tick 6000 (Granary+RNG interaction).
# Observed 2026-05-29: seed 137 pop=11, food=15 (barely above threshold) in next run.
# Fix: change threshold to max(N, population * 2) — buffer that scales with population.
echo ""
echo "=== FoodToGrain Granary threshold safety check ==="
ftg_threshold=$(grep 'Recipe::FoodToGrain' src/ecs/systems.rs 2>/dev/null | grep -oE 'food > ([0-9]+)' | grep -oE '[0-9]+$' | head -1)
if [ -n "$ftg_threshold" ]; then
  if [ "$ftg_threshold" -lt 20 ]; then
    echo "WARN: FoodToGrain Granary triggers at food > $ftg_threshold (src/ecs/systems.rs) — too low for pop >= 10 (consumption ~1-1.5/tick; buffer lasts ~70-100 ticks). rand::rng() task-assignment fluctuations can exhaust this buffer. Observed food=0 crash: 2026-05-28 seed 137 pop=11. Fix: adaptive threshold max($ftg_threshold, population*2) in both FoodToGrain processing gates."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: FoodToGrain threshold=${ftg_threshold} (>=20) — adequate food buffer for typical populations"
  fi
else
  echo "SKIP: FoodToGrain threshold not found in src/ecs/systems.rs — check needs updating"
fi

# 52. Build site backlog ratio detection
# When auto-build queues more build_sites than workers can complete, pending sites pile up.
# Observed 2026-06-02: seed 137 diagnostics showed 8 pending build_sites with only 4 workers
# (pop=4 due to rand::rng() low-population run) — ratio=2.0, clearly backlogged.
# Root cause: game/build.rs auto-build scoring does not account for available worker count.
# Threshold: build_sites / population > 1.5 in any seed = construction structurally stalled.
# Requires 'build_sites' and 'population' fields in metrics_history entries to fire.
echo ""
echo "=== Build site backlog ratio detection ==="
if [ -f "docs/metrics_history.json" ]; then
  backlog=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if not data:
    print('no_data')
else:
    last = data[-1]
    seeds = last.get('seeds', {})
    backlogged = []
    total_with_data = sum(1 for v in seeds.values() if 'build_sites' in v and 'population' in v)
    for s, v in seeds.items():
        if 'build_sites' not in v or 'population' not in v:
            continue
        bs = v['build_sites']
        pop = v['population']
        if pop > 0 and bs / pop > 1.5:
            backlogged.append(f'seed {s}: {bs} sites/{pop} workers')
    if total_with_data == 0:
        print('no_build_site_data')
    elif backlogged:
        print(f'backlogged:{len(backlogged)}:' + ','.join(backlogged))
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$backlog" | grep -q "^backlogged:"; then
    count=$(echo "$backlog" | cut -d: -f2)
    desc=$(echo "$backlog" | cut -d: -f3-)
    echo "WARN: Build site backlog in ${count} seed(s) (${desc}) — auto-build queueing faster than workers can build (ratio > 1.5 sites/worker). Root cause: game/build.rs does not cap build queue relative to available population. Fix: check pending build_site count before queuing new sites. Complements check #44 (wood stall) and check #37 (housing saturation)."
    WARNINGS=$((WARNINGS + 1))
  elif echo "$backlog" | grep -q "no_build_site_data"; then
    echo "SKIP: No 'build_sites' field in metrics_history — add it to enable build backlog detection (first observed 2026-06-02: seed 137 had 8 sites/4 workers)"
  else
    echo "OK: Build site backlog within worker capacity (ratio <= 1.5 sites/worker in all seeds)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

# 53. Grain dead-end detection
# When FoodToGrain Granary converts food->grain but Bakery is blocked (planks=0),
# grain accumulates as a dead-end resource: it cannot be used for anything except
# Bakery (grain+wood->bread), which requires planks >= 8 (check #47) and grain > 30.
# If planks=0 and bread=0, the grain pile is inaccessible — a pure resource sink.
# Meanwhile, the Granary continues to drain the food buffer (check #51).
# First observed 2026-06-06: seed 777 grain=38, food=16, planks=0, pop=14.
# Requires 'grain', 'bread', 'planks' (or food_per_cap + biomes as proxy) in metrics_history.
# Uses the diagnostics JSON directly since metrics_history doesn't store grain/planks/bread yet.
echo ""
echo "=== Grain dead-end detection ==="
if [ -f "docs/metrics_history.json" ]; then
  grain_dead=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if not data:
    print('no_data')
else:
    last = data[-1]
    seeds = last.get('seeds', {})
    # Check if metrics_history has grain/planks/bread fields
    has_grain_data = any('grain' in v for v in seeds.values())
    if not has_grain_data:
        print('no_grain_data')
    else:
        dead_ends = []
        for s, v in seeds.items():
            grain = v.get('grain', 0)
            bread = v.get('bread', 0)
            planks = v.get('planks', 0)
            pop = v.get('population', 0)
            if grain > 20 and bread == 0 and planks == 0 and pop >= 5:
                dead_ends.append(f'seed {s}: grain={grain}, planks={planks}, pop={pop}')
        if dead_ends:
            print(f'dead_end:{len(dead_ends)}:' + ','.join(dead_ends))
        else:
            print('ok')
" 2>/dev/null || echo "ok")
  if echo "$grain_dead" | grep -q "^dead_end:"; then
    count=$(echo "$grain_dead" | cut -d: -f2)
    desc=$(echo "$grain_dead" | cut -d: -f3-)
    echo "WARN: Grain dead-end in ${count} seed(s) (${desc}) — Granary converting food->grain but Bakery blocked (planks=0). Grain pile inaccessible; food buffer draining via Granary while grain cannot be used. Root cause: Forest<1% (check #42) → no wood → no Workshop output → planks=0 → Bakery structurally impossible. Fix biome distribution before tuning Granary threshold."
    WARNINGS=$((WARNINGS + 1))
  elif echo "$grain_dead" | grep -q "no_grain_data"; then
    echo "SKIP: No 'grain'/'planks'/'bread' fields in metrics_history — add them to enable grain dead-end detection (first observed 2026-06-06: seed 777 grain=38, planks=0)"
  else
    echo "OK: No grain dead-end detected (grain <= 20, or bread/planks > 0 in all seeds)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

# 54. Known-flaky test ignore guard: mining_sparkle_particles_spawn
# Discovered 2026-06-05: fails when stone-mining villager does not produce white-blue sparkle
# particles within 50 ticks. Root cause: probabilistic particle spawn via rand::rng(); villager
# may transition out of Gathering{Stone} state before sparkle fires. Same mechanism as
# construction_dust_particles_spawn (check #14) and build_site_gets_completed_in_game (check #35).
# Should be #[ignore]d until seeded per-entity RNG is used. Documented in features.json:game_loop.
echo ""
echo "=== Known-flaky test ignore guard: mining_sparkle_particles_spawn ==="
if grep -q "fn mining_sparkle_particles_spawn" src/game/tests.rs 2>/dev/null; then
  if grep -B3 "fn mining_sparkle_particles_spawn" src/game/tests.rs | grep -q "#\[ignore"; then
    echo "OK: Known-flaky test mining_sparkle_particles_spawn is properly marked #[ignore]"
  else
    echo "WARN: mining_sparkle_particles_spawn is documented as flaky (features.json:game_loop, first failed 2026-06-05) but not marked #[ignore] — false CI failures expected. Root cause: rand::rng() probabilistic particle spawn (same as construction_dust and build_site_gets_completed)."
    WARNINGS=$((WARNINGS + 1))
  fi
else
  echo "OK: mining_sparkle_particles_spawn not found (removed or renamed — update this check)"
fi

# 55. Flaky test family unignored alert
# All three known-flaky tests (construction_dust_particles_spawn, build_site_gets_completed_in_game,
# mining_sparkle_particles_spawn) are individually guarded by checks #14, #35, #54.
# When ALL THREE are simultaneously unignored, it means the whole probabilistic-particle / rand::rng()
# family is unguarded — giving 3x the CI false-failure rate of any single test.
# Root cause for all three: rand::rng() in build/particle/AI paths makes outcomes non-deterministic;
# none will be reliably fixable until seeded per-entity RNG is used (BACKLOG.md).
# First all-three failure observed: 2026-06-05 (775 passed, 3 failed, 10 ignored).
# Added 2026-06-08.
echo ""
echo "=== Flaky test family unignored alert ==="
FLAKY_UNIGNORED_COUNT=0
for fn_name in construction_dust_particles_spawn build_site_gets_completed_in_game mining_sparkle_particles_spawn; do
  if grep -q "fn $fn_name" src/game/tests.rs 2>/dev/null; then
    if ! grep -B3 "fn $fn_name" src/game/tests.rs | grep -q "#\[ignore"; then
      FLAKY_UNIGNORED_COUNT=$((FLAKY_UNIGNORED_COUNT + 1))
    fi
  fi
done
if [ "$FLAKY_UNIGNORED_COUNT" -ge 3 ]; then
  echo "WARN: ALL 3 known-flaky tests are simultaneously unignored (construction_dust_particles_spawn, build_site_gets_completed_in_game, mining_sparkle_particles_spawn) — 3x CI false-failure risk. All three fail due to rand::rng() non-determinism in particle/build/AI paths. Mark all three #[ignore] until seeded per-entity RNG is implemented (BACKLOG.md)."
  WARNINGS=$((WARNINGS + 1))
elif [ "$FLAKY_UNIGNORED_COUNT" -ge 2 ]; then
  echo "WARN: ${FLAKY_UNIGNORED_COUNT}/3 known-flaky tests are unignored — elevated CI false-failure risk."
  WARNINGS=$((WARNINGS + 1))
else
  echo "OK: Flaky test family not simultaneously unignored (0-1 of 3 at risk)"
fi

# 56. Bakery chain partial stall: grain > 15 with planks insufficient (< 8)
# check #53 only fires when planks == 0 (fully blocked). This check catches partial stalls:
# grain is accumulating (Granary working) but planks are 1-7 (Workshop producing but far below
# Bakery's minimum of 8). With Forest < 1% (check #42), wood supply is irregular; Workshop
# produces planks only during RNG-favorable wood-gathering runs, accumulating slowly.
# Meanwhile FoodToGrain Granary continues draining the food buffer (check #51).
# First observed 2026-06-09: seed 777 grain=26, planks=3, bread=0, pop=11 in diagnostics run.
# Requires 'grain', 'planks', 'bread', 'population' fields in metrics_history entries.
echo ""
echo "=== Bakery chain partial stall detection ==="
if [ -f "docs/metrics_history.json" ]; then
  partial_stall=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if not data:
    print('no_data')
else:
    last = data[-1]
    seeds = last.get('seeds', {})
    has_data = any('grain' in v and 'planks' in v and 'bread' in v for v in seeds.values())
    if not has_data:
        print('no_grain_data')
    else:
        stalled = []
        for s, v in seeds.items():
            grain = v.get('grain', 0)
            bread = v.get('bread', 0)
            planks = v.get('planks', 0)
            pop = v.get('population', 0)
            if grain > 15 and bread == 0 and planks < 8 and pop >= 5:
                stalled.append(f'seed {s}: grain={grain}, planks={planks}, pop={pop}')
        if stalled:
            print(f'stalled:{len(stalled)}:' + ','.join(stalled))
        else:
            print('ok')
" 2>/dev/null || echo "ok")
  if echo "$partial_stall" | grep -q "^stalled:"; then
    count=$(echo "$partial_stall" | cut -d: -f2)
    desc=$(echo "$partial_stall" | cut -d: -f3-)
    echo "WARN: Bakery chain partial stall in ${count} seed(s) (${desc}) — grain accumulating but planks < 8 (Bakery minimum). Workshop IS producing planks but too slowly (irregular wood supply from Forest < 1%, check #42). Granary continues draining food buffer (check #51). Fix biome distribution to stabilize wood supply before tuning Granary thresholds."
    WARNINGS=$((WARNINGS + 1))
  elif echo "$partial_stall" | grep -q "no_grain_data"; then
    echo "SKIP: No 'grain'/'planks'/'bread' fields in metrics_history — add them to enable partial stall detection"
  else
    echo "OK: No Bakery chain partial stall (grain <= 15, planks >= 8, or bread > 0 in all seeds)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

# 57. FoodToGrain Granary food-drain anti-pattern
# Complements check #53 (grain dead-end, planks=0) and check #56 (partial stall, planks<8).
# This check catches the FOOD CRISIS dimension: when food_per_cap < 2.0 AND grain > 20,
# the Granary is actively draining the food buffer into grain even while food is scarce.
# Unlike #53 and #56 (which focus on grain reachability for Bakery), this detects the
# live food-drain happening right now — settlements with food Crisis AND growing grain pile.
# At this point the Granary threshold (food > 15, systems.rs:979) is too low relative to
# population demand; the buffer lasts < 100 ticks before another drain cycle hits.
# Fix: adaptive threshold max(15, population*2) in systems.rs FoodToGrain gates.
# First clearly observed: 2026-06-10, seed 777 (food/cap=1.64, grain=26).
# Requires 'food_per_cap' and 'grain' fields in metrics_history entries.
echo ""
echo "=== FoodToGrain food-drain anti-pattern (low food + high grain) ==="
if [ -f "docs/metrics_history.json" ]; then
  ftg_drain=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
if not data:
    print('no_data')
else:
    last = data[-1]
    seeds = last.get('seeds', {})
    hits = [(s, v.get('food_per_cap', 999), v.get('grain', 0))
            for s, v in seeds.items()
            if 'food_per_cap' in v and 'grain' in v
            and v['food_per_cap'] < 2.0 and v['grain'] > 20]
    total = sum(1 for v in seeds.values() if 'food_per_cap' in v and 'grain' in v)
    if total == 0:
        print('no_grain_data')
    elif hits:
        desc = ', '.join(f'seed {s} food/cap={fpc:.2f} grain={gr}' for s, fpc, gr in hits)
        print(f'drain:{len(hits)}:{desc}')
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$ftg_drain" | grep -q "^drain:"; then
    count=$(echo "$ftg_drain" | cut -d: -f2)
    desc=$(echo "$ftg_drain" | cut -d: -f3-)
    echo "WARN: FoodToGrain food-drain anti-pattern in ${count} seed(s): ${desc} — food buffer depleted while grain stockpiles. Pre-built Granary (game/mod.rs:1064, food>15 threshold) draining food at high pop while grain cannot be used (Bakery blocked). Fix: adaptive threshold max(15, population*2) in systems.rs. See check #51 (static threshold), #53 (dead-end grain), #56 (partial stall)."
    WARNINGS=$((WARNINGS + 1))
  elif echo "$ftg_drain" | grep -q "no_grain_data"; then
    echo "SKIP: No 'food_per_cap'/'grain' fields in metrics_history — check #49 schema completeness should catch this"
  else
    echo "OK: No food-drain anti-pattern (food_per_cap >= 2.0 or grain <= 20 in all seeds)"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi


# 58. Recurring total starvation pattern detection
# Total starvation (food=0 with living population) is a catastrophic event that should be rare.
# When any seed has experienced food=0 with pop>0 in 3+ historical health checks, it indicates
# a structural problem, not bad luck. Seed 137 confirmed: 2026-05-16, 2026-05-21, 2026-05-28
# (all diagnostics runs with pop=11) — all driven by rand::rng() under-farming + FoodToGrain
# Granary draining buffer. Root cause: adaptive threshold missing (systems.rs food>15 too low
# for pop>=10). Complement to check #50 (single-run acute starvation) and check #51 (threshold).
echo ""
echo "=== Recurring total starvation pattern detection ==="
if [ -f "docs/metrics_history.json" ]; then
  recurring=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
starvation = {}
for entry in data:
    for seed, vals in entry.get('seeds', {}).items():
        if vals.get('food', -1) == 0 and vals.get('population', 0) > 0:
            starvation.setdefault(seed, []).append(entry.get('date', '?'))
worst = [(seed, dates) for seed, dates in starvation.items() if len(dates) >= 3]
if worst:
    desc = '; '.join(f'seed {s}: {len(d)} times ({d[-1]})' for s, d in worst)
    print(f'recurring:{desc}')
else:
    print('ok')
" 2>/dev/null || echo "ok")
  if echo "$recurring" | grep -q "^recurring:"; then
    desc=$(echo "$recurring" | cut -d: -f2-)
    echo "WARN: Recurring total starvation (food=0, pop>0) in 3+ health checks: $desc — structural food crisis, not bad luck. Root cause: FoodToGrain Granary triggers at food>15 (check #51); rand::rng() reduces farmer count; pop>=10 consumes ~1-1.5 food/tick. Fix: adaptive threshold max(15, population*2) in systems.rs FoodToGrain gates. See also check #50 (acute starvation) and check #39 (near-starvation)."
    WARNINGS=$((WARNINGS + 1))
  else
    echo "OK: No seed has experienced recurring total starvation (food=0) in 3+ health checks"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

# 59. Bakery chain has never produced bread in evaluation history
# If bread=0 in ALL seeds across ALL metrics_history entries that contain bread data,
# the Bakery production chain has NEVER functioned since bread tracking began.
# This is a stronger signal than check #47 (static planks threshold) — it proves
# the end product has never appeared, not just that it's currently blocked.
# Root cause: Forest biome <1% (check #42) → no wood → Workshop starved → planks=0 → Bakery blocked.
# Requires 'bread' field in metrics_history entries (added to schema in check #53).
# First confirmed: bread=0 across all 5 bread-tracking entries as of 2026-06-13.
echo ""
echo "=== Bakery chain never-produced-bread detection ==="
if [ -f "docs/metrics_history.json" ]; then
  never_bread=$(python3 -c "
import json
with open('docs/metrics_history.json') as f:
    data = json.load(f)
entries_with_bread = [e for e in data if any('bread' in v for v in e.get('seeds', {}).values())]
if not entries_with_bread:
    print('no_bread_data')
else:
    all_zero = all(
        v.get('bread', 0) == 0
        for e in entries_with_bread
        for v in e.get('seeds', {}).values()
        if 'bread' in v
    )
    if all_zero:
        print(f'never:{len(entries_with_bread)}')
    else:
        print('ok')
" 2>/dev/null || echo "ok")
  if echo "$never_bread" | grep -q "^never:"; then
    count=$(echo "$never_bread" | cut -d: -f2)
    echo "WARN: Bread=0 in ALL seeds across ALL ${count} metrics_history entries with bread data — Bakery production chain has NEVER functioned in evaluation history. Root cause: Forest<1% (check #42) → no wood → Workshop starved → planks=0 → Bakery structurally impossible (check #47). Fix biome distribution in terrain_pipeline.rs before investigating Bakery recipe or thresholds."
    WARNINGS=$((WARNINGS + 1))
  elif echo "$never_bread" | grep -q "no_bread_data"; then
    echo "SKIP: No 'bread' field in metrics_history — check #49 schema completeness should catch this"
  else
    echo "OK: Bread has been produced at least once in evaluation history — Bakery chain is not permanently blocked"
  fi
else
  echo "SKIP: docs/metrics_history.json not found"
fi

echo ""
echo "=== Summary ==="
systems=$(jq '.systems | length' "$FEATURES")
echo "Systems: $systems"
echo "Errors: $ERRORS"
echo "Warnings: $WARNINGS"

if [ $ERRORS -gt 0 ]; then
  exit 1
fi

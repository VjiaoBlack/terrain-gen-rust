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

# Summary
echo ""
echo "=== Summary ==="
systems=$(jq '.systems | length' "$FEATURES")
echo "Systems: $systems"
echo "Errors: $ERRORS"
echo "Warnings: $WARNINGS"

if [ $ERRORS -gt 0 ]; then
  exit 1
fi

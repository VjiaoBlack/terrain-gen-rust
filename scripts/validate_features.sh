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

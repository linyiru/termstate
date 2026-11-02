#!/usr/bin/env bash
# Differential test: run spec-tests/vectors/parse.json through Ghostty's
# OSC 7501 parser (libghostty-vt) and compare with the vectors.
#
# Needs zig matching Ghostty's minimum_zig_version (0.17.0 at the pinned
# ref) and ~1.5 GB for the checkout and build cache.
#
# Usage: tools/ghostty-diff/run.sh [ghostty-ref]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$here/../.."
ref="${1:-0b1729e945978fca4bcd6165d8ac408bf675a7f2}"
src="$root/.cache/ghostty"

if [[ ! -d "$src/.git" ]]; then
  git init -q "$src"
  git -C "$src" remote add origin https://github.com/ghostty-org/ghostty.git
fi
git -C "$src" fetch -q --depth 1 origin "$ref"
git -C "$src" checkout -q --force FETCH_HEAD
echo "ghostty $(git -C "$src" rev-parse --short HEAD)"

parsers="$src/src/terminal/osc/parsers"
cat "$here/harness.zig" >> "$parsers/program_status.zig"
python3 -I -c '
import json, sys
cases = json.load(open(sys.argv[1]))["cases"]
open(sys.argv[2], "w").write("".join("x" + c["body"].encode().hex() + "\n" for c in cases))
' "$root/spec-tests/vectors/parse.json" "$parsers/termstate_diff_input.txt"

log="$root/.cache/ghostty-diff.log"
# The test "fails" because Ghostty logs warnings for discarded reports;
# the TSDIFF lines are what matter.
(cd "$src" && zig build test-lib-vt -Dtest-filter="termstate differential" >"$log" 2>&1) || true
git -C "$src" checkout -q -- . && rm -f "$parsers/termstate_diff_input.txt"

python3 -I "$here/compare.py" "$root/spec-tests/vectors/parse.json" "$log"

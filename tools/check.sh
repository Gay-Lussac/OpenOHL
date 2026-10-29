#!/bin/sh
# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.
#
# The full local check before a push (CI is started by hand: this is what
# keeps `main` green). Runs what CI runs, stopping at the first failure:
# formatting, both clippy runs, the WebAssembly plugins the tests need,
# every test (plugin and GPU tests required, not skipped), rustdoc, the
# book, the documentation links, the validation benchmarks. Prints one
# line per step (`fmt-ok`, `clippy1-ok`, …, `215 cases: 215 passed, 0
# failed`) and `check-ok` at the end.
#
#   sh tools/check.sh [TREE]
#
# TREE: the checkout to check (default: this one), e.g. a worktree of the
# commit to push, so work can go on meanwhile. Set CARGO_TARGET_DIR to build
# elsewhere than TREE/target. Expect 30 to 60 minutes from a cold build,
# much less after.
#
# Private parts, only where they are (in local-data/, never committed):
# - local-data/check/plugins.txt: private plugin folders (one per line,
#   relative to this checkout), built for wasm32-wasip2, tested and
#   linted;
# - local-data/check/scan.txt: extended regular expressions (one per
#   line, `#` comments) that must not appear in what is pushed (names of
#   real lines, persons, companies): the diff from BASE (default
#   origin/main) to TREE's HEAD is searched; local-data/check/scan-ok.txt
#   the same for the lines to let through.
# Build folders are pruned at the end (tools/prune-target.sh).
set -e
HERE=$(cd "$(dirname "$0")/.." && pwd)
TREE=$(cd "${1:-$HERE}" && pwd)
C=${CARGO:-cargo}
PRIVATE="$HERE/local-data/check"
OUT=${CARGO_TARGET_DIR:-$TREE/target}
mkdir -p "$OUT"
LOG="$OUT/check-test.log"
cd "$TREE"

# The real-line scan first: nothing else matters if it fails.
if [ -f "$PRIVATE/scan.txt" ]; then
    BASE=${BASE:-origin/main}
    PAT=$(grep -v '^#' "$PRIVATE/scan.txt" | grep -v '^$' | paste -sd '|' -)
    OK='^[0-9]*:@@'
    [ -f "$PRIVATE/scan-ok.txt" ] && OK="$OK|$(grep -v '^#' "$PRIVATE/scan-ok.txt" | grep -v '^$' | paste -sd '|' -)"
    HITS=$(git diff "$BASE"..HEAD | grep -n -i -E "$PAT" | grep -v -E "$OK" || true)
    if [ -n "$HITS" ]; then
        echo "$HITS" | head -20
        echo scan-FAILED
        exit 1
    fi
    echo scan-ok
fi

$C fmt --all --check && echo fmt-ok || { echo fmt-FAILED; exit 1; }
$C clippy -q --workspace --exclude ohl-py --all-targets -- -D warnings && echo clippy1-ok || { echo clippy1-FAILED; exit 1; }
$C clippy -q -p ohl-app --no-default-features --all-targets -- -D warnings && echo clippy2-ok || { echo clippy2-FAILED; exit 1; }

# The plugins the tests load: each built in its own folder (as CI does).
for d in plugins/examples/* plugins/tests/* plugins/codes/*; do
    [ -f "$d/Cargo.toml" ] || continue
    (cd "$d" && env -u CARGO_TARGET_DIR $C build -q --release --target wasm32-wasip2) || { echo "plugins-FAILED $d"; exit 1; }
done
echo plugins-built

OHL_REQUIRE_PLUGIN_TESTS=1 OHL_REQUIRE_GPU_TESTS=1 $C test -q --no-fail-fast --workspace --exclude ohl-py > "$LOG" 2>&1 || {
    grep -E "FAILED|panicked|^error" "$LOG" | head -30
    echo "tests-FAILED (all output: $LOG)"
    exit 1
}
echo "tests-ok ($(grep -c "test result: ok" "$LOG") test binaries)"

RUSTDOCFLAGS="-D warnings" $C doc -q --no-deps --workspace --exclude ohl-py && echo doc-ok || { echo doc-FAILED; exit 1; }
mdbook build > /dev/null 2>&1 && echo mdbook-ok || { echo mdbook-FAILED; exit 1; }
python3 -I tools/check_doc_links.py > /dev/null && echo links-ok || { echo links-FAILED; exit 1; }

# The benchmarks read the shipped libraries next to the program: with the
# build outside the tree, a link to them there.
if [ -n "$CARGO_TARGET_DIR" ] && [ ! -e "$(dirname "$OUT")/libraries" ]; then
    ln -s "$TREE/libraries" "$(dirname "$OUT")/libraries"
fi
$C build -q -p ohl-cli
BENCH=$("$OUT/debug/ohl" validate benchmarks 2>&1 | tail -1)
echo "$BENCH"
case "$BENCH" in *" 0 failed"*) ;; *) echo benchmarks-FAILED; exit 1 ;; esac

if [ -f "$PRIVATE/plugins.txt" ]; then
    for d in $(grep -v '^#' "$PRIVATE/plugins.txt"); do
        d="$HERE/$d"
        (cd "$d" && env -u CARGO_TARGET_DIR $C build -q --release --target wasm32-wasip2 \
            && env -u CARGO_TARGET_DIR $C test -q 2>&1 | grep "test result" \
            && env -u CARGO_TARGET_DIR $C clippy -q --target wasm32-wasip2 -- -D warnings) \
            && echo "$(basename "$d")-ok" || { echo "$(basename "$d")-FAILED"; exit 1; }
    done
fi

sh "$HERE/tools/prune-target.sh" "$OUT"
[ "$OUT" = "$HERE/target" ] || [ ! -d "$HERE/target" ] || sh "$HERE/tools/prune-target.sh" "$HERE/target"
echo check-ok

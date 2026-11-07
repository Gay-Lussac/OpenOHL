#!/bin/sh
# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.
#
# The full local check before a push (CI is started by hand: this is what
# keeps `main` green). Runs what CI runs, stopping at the first failure:
# formatting, both clippy runs, the WebAssembly plugins the tests need,
# every test (plugin and GPU tests required, not skipped), rustdoc, the
# book, the documentation links, the validation benchmarks, every plugin
# checked (`ohl plugin check`) and a plugin of every kind built from its
# template. Prints one
# line per step (`fmt-ok`, `clippy1-ok`, …, `218 cases: 218 passed, 0
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
# No debug information in the check's builds (owner, 2026-10-29: no
# unnecessary disk use): on macOS the line tables keep every object file
# beside the binaries for the debugger (some 30 GB); a failing test still
# says its file and line. CARGO_PROFILE_DEV_DEBUG set before keeps its
# value.
export CARGO_PROFILE_DEV_DEBUG="${CARGO_PROFILE_DEV_DEBUG:-false}"
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
    # The lines added only: a line removed (a leak taken out) is already
    # public and is not pushed again.
    HITS=$(git diff "$BASE"..HEAD | grep -n -i -E "$PAT" | grep -E '^[0-9]+:\+' | grep -v -E '^[0-9]+:\+\+\+ ' | grep -v -E "$OK" || true)
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

# The tests and the benchmarks read the shipped libraries next to the
# program: with the build outside the tree, a link to them there (made
# again when it went: 2026-10-29, a cleanup removed it mid-run).
if [ -n "${CARGO_TARGET_DIR:-}" ] && [ ! -e "$(dirname "$OUT")/libraries" ]; then
    ln -sfn "$TREE/libraries" "$(dirname "$OUT")/libraries"
fi

# The plugins the tests load: each built in its own folder (as CI does),
# formatted and linted as the workspace is (2026-11-03).
for d in plugins/examples/* plugins/tests/* plugins/codes/*; do
    [ -f "$d/Cargo.toml" ] || continue
    (cd "$d" && $C fmt --check) || { echo "plugins-fmt-FAILED $d"; exit 1; }
    (cd "$d" && env -u CARGO_TARGET_DIR $C build -q --release --target wasm32-wasip2) || { echo "plugins-FAILED $d"; exit 1; }
    (cd "$d" && env -u CARGO_TARGET_DIR $C clippy -q --release --target wasm32-wasip2 -- -D warnings) || { echo "plugins-clippy-FAILED $d"; exit 1; }
done
echo plugins-built

# The plugins' own native tests (plain Rust, beside their WebAssembly
# build; built in the check's target folder, pruned with it).
for d in plugins/examples/building-cuts; do
    (cd "$d" && $C fmt --check && CARGO_TARGET_DIR="$OUT" $C test -q > /dev/null) || { echo "plugin-tests-FAILED $d"; exit 1; }
done
echo plugin-tests-ok

OHL_REQUIRE_PLUGIN_TESTS=1 OHL_REQUIRE_GPU_TESTS=1 $C test -q --no-fail-fast --workspace --exclude ohl-py > "$LOG" 2>&1 || {
    grep -E "FAILED|panicked|^error" "$LOG" | head -30
    echo "tests-FAILED (all output: $LOG)"
    exit 1
}
echo "tests-ok ($(grep -c "test result: ok" "$LOG") test binaries)"

RUSTDOCFLAGS="-D warnings" $C doc -q --no-deps --workspace --exclude ohl-py && echo doc-ok || { echo doc-FAILED; exit 1; }
mdbook build > /dev/null 2>&1 && echo mdbook-ok || { echo mdbook-FAILED; exit 1; }
python3 -I tools/check_doc_links.py > /dev/null && echo links-ok || { echo links-FAILED; exit 1; }

$C build -q -p ohl-cli
BENCH=$("$OUT/debug/ohl" validate benchmarks 2>&1 | tail -1)
echo "$BENCH"
case "$BENCH" in *" 0 failed"*) ;; *) echo benchmarks-FAILED; exit 1 ;; esac

if [ -f "$PRIVATE/plugins.txt" ]; then
    for d in $(grep -v '^#' "$PRIVATE/plugins.txt"); do
        d="$HERE/$d"
        (cd "$d" && env -u CARGO_TARGET_DIR $C build -q --release --target wasm32-wasip2 \
            && env -u CARGO_TARGET_DIR $C test -q 2>&1 | grep "test result" \
            && env -u CARGO_TARGET_DIR $C clippy -q --target wasm32-wasip2 -- -D warnings \
            && "$OUT/debug/ohl" plugin check . > /dev/null) \
            && echo "$(basename "$d")-ok" || { echo "$(basename "$d")-FAILED"; exit 1; }
    done
fi

# Every plugin of the repository as the applications will use it (ADR-0012).
PLUGIN_DIRS=$(for d in plugins/examples/* plugins/tests/* plugins/codes/*; do
    [ -f "$d/ohl-plugin.toml" ] && echo "$d"
done)
# shellcheck disable=SC2086
"$OUT/debug/ohl" plugin check $PLUGIN_DIRS > "$OUT/plugin-check.log" 2>&1 \
    && echo plugins-checked || { grep -B3 FAILED "$OUT/plugin-check.log" | head -30; echo plugin-check-FAILED; exit 1; }

# A plugin of every kind from its template: written, formatted, built and
# linted on its own, and checked (ADR-0012: the templates stay usable).
TEMPLATES="$OUT/plugin-templates"
rm -rf "${TEMPLATES:?}"
for k in minimal design-code check io terrain project sheet-templates wire-colours system-styles member-capacity field-presets; do
    t="$TEMPLATES/$k"
    ("$OUT/debug/ohl" plugin new "my-$k" --kind "$k" --out "$t" > /dev/null \
        && cd "$t" && $C fmt --check \
        && env -u CARGO_TARGET_DIR $C build -q --release --target wasm32-wasip2 \
        && env -u CARGO_TARGET_DIR $C clippy -q --release --target wasm32-wasip2 -- -D warnings \
        && "$OUT/debug/ohl" plugin check . > /dev/null) || { echo "plugin-template-FAILED $k"; exit 1; }
done
rm -rf "${TEMPLATES:?}"
echo plugin-templates-ok

sh "$HERE/tools/prune-target.sh" "$OUT"
[ "$OUT" = "$HERE/target" ] || [ ! -d "$HERE/target" ] || sh "$HERE/tools/prune-target.sh" "$HERE/target"
echo check-ok

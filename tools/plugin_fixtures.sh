#!/bin/sh
# This file is part of OpenOHL. Copyright (C) 2026 the OpenOHL contributors.
# SPDX-License-Identifier: AGPL-3.0-or-later (GNU Affero General Public
# License v3.0 or later); see the LICENSE file or https://www.gnu.org/licenses/.
#
# Builds the plugin binaries the compatibility tests load
# (crates/ohl-plugins/tests/compat-0.3/, ADR-0012): one plugin of each world
# of interface 0.3, as a plugin author built it, kept in the repository so
# that a change of the host that would break a plugin already built fails
# the tests. Run it only to add a fixture or when a fixture's plugin source
# changes on purpose; never to make a failing compatibility test pass.
#
#   sh tools/plugin_fixtures.sh
#
# Each fixture is its plugin's manifest (its entry renamed plugin.wasm), the
# files the manifest names, and the WebAssembly built with local paths
# mapped away (no home folder or login name in the binary).
set -eu
cd "$(dirname "$0")/.."
OUT=crates/ohl-plugins/tests/compat-0.3
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

# name  source folder  (the template's check plugin is made by `ohl plugin new`)
FIXTURES="
hello-wasm plugins/examples/hello-wasm
geojson plugins/examples/geojson
esri-grid plugins/examples/esri-grid
project-bundle plugins/examples/project-bundle
code-old plugins/tests/code-old
code-presets plugins/tests/code-presets
sheet-presets plugins/tests/sheet-presets
wire-colours plugins/tests/wire-colours
member-capacity plugins/tests/member-capacity
field-presets plugins/tests/field-presets
"

cargo build -q -p ohl-cli
OHL=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')/debug/ohl
"$OHL" plugin new compat-check --kind check --out "$WORK/compat-check" > /dev/null

build() { # name folder
    name=$1
    src=$2
    rm -rf "${OUT:?}/$name"
    mkdir -p "$OUT/$name"
    (cd "$src" && env -u CARGO_TARGET_DIR CARGO_TARGET_DIR="$WORK/target-$name" \
        RUSTFLAGS="--remap-path-prefix=$HOME=~ --remap-path-prefix=$PWD=." \
        cargo build -q --release --target wasm32-wasip2)
    wasm=$(ls "$WORK/target-$name"/wasm32-wasip2/release/*.wasm | head -1)
    cp "$wasm" "$OUT/$name/plugin.wasm"
    sed 's|^entry = .*|entry = "plugin.wasm"|' "$src/ohl-plugin.toml" > "$OUT/$name/ohl-plugin.toml"
    # The files the manifest names (sheet templates, plan sets, class schemes).
    for f in $(grep -E '^file *=' "$src/ohl-plugin.toml" | sed 's/.*= *"\(.*\)"/\1/'); do
        mkdir -p "$OUT/$name/$(dirname "$f")"
        cp "$src/$f" "$OUT/$name/$f"
    done
    if strings "$OUT/$name/plugin.wasm" | grep -q -e "$HOME" -e "$(id -un)"; then
        echo "fixture $name holds a local path or the login name" >&2
        exit 1
    fi
    echo "$name: $(wc -c < "$OUT/$name/plugin.wasm") bytes"
}

echo "$FIXTURES" | while read -r name src; do
    if [ -n "$name" ]; then
        build "$name" "$src"
    fi
done
build compat-check "$WORK/compat-check"
